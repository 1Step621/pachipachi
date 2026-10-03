use std::{
    collections::{HashMap, HashSet},
    fs::{self, OpenOptions},
    io,
    os::{fd::AsFd, unix::fs::OpenOptionsExt},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use anyhow::{Context as _, Result, ensure};
use async_channel::{Receiver, Sender};
use evdev::{
    AttributeSetRef, EventSummary, InputEvent, KeyCode, SynchronizationCode, raw_stream::RawDevice,
};
use nix::poll::{PollFd, PollFlags, poll};

use crate::keys::{KeyChord, Modifier, Modifiers};

const RESCAN_INTERVAL: Duration = Duration::from_secs(1);

pub fn is_keyboard_key(key: KeyCode) -> bool {
    key != KeyCode::KEY_RESERVED && format!("{key:?}").starts_with("KEY_")
}

fn is_keyboard(keys: Option<&AttributeSetRef<KeyCode>>) -> bool {
    keys.is_some_and(|keys| {
        [
            KeyCode::KEY_A,
            KeyCode::KEY_ENTER,
            KeyCode::KEY_KPENTER,
            KeyCode::KEY_SPACE,
        ]
        .into_iter()
        .any(|key| keys.contains(key))
    })
}

struct Keyboard {
    device: RawDevice,
    filter: KeyFilter,
}

#[derive(Default)]
struct KeyFilter {
    dropped: bool,
    held_modifiers: HashSet<KeyCode>,
}

impl KeyFilter {
    fn resync(&mut self, keys: impl IntoIterator<Item = KeyCode>) {
        self.held_modifiers = keys
            .into_iter()
            .filter(|key| Modifier::from_key(*key).is_some())
            .collect();
    }

    fn modifiers(&self) -> Modifiers {
        Modifiers::from_keys(self.held_modifiers.iter().copied())
    }

    fn keypress(&mut self, event: InputEvent) -> Option<KeyCode> {
        // RawDevice avoids counting synthetic state-recovery events as real presses.
        match event.destructure() {
            EventSummary::Synchronization(_, SynchronizationCode::SYN_DROPPED, _) => {
                self.dropped = true;
                self.held_modifiers.clear();
            }
            EventSummary::Synchronization(_, SynchronizationCode::SYN_REPORT, _) => {
                self.dropped = false
            }
            EventSummary::Key(_, key, value)
                if !self.dropped && Modifier::from_key(key).is_some() =>
            {
                match value {
                    1 => {
                        self.held_modifiers.insert(key);
                    }
                    0 => {
                        self.held_modifiers.remove(&key);
                    }
                    _ => {}
                }
            }
            EventSummary::Key(_, key, 1) if !self.dropped && is_keyboard_key(key) => {
                return Some(key);
            }
            _ => {}
        }
        None
    }
}

fn event_paths() -> io::Result<Vec<PathBuf>> {
    let mut paths = fs::read_dir("/dev/input")?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("event"))
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    paths.sort();
    Ok(paths)
}

fn open_keyboard(path: &Path) -> io::Result<Option<Keyboard>> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(nix::libc::O_NONBLOCK)
        .open(path)?;
    let device = RawDevice::try_from(file)?;
    if !is_keyboard(device.supported_keys()) {
        return Ok(None);
    }
    let mut filter = KeyFilter::default();
    filter.resync(device.get_key_state()?.iter());
    Ok(Some(Keyboard { device, filter }))
}

#[derive(Default)]
struct Devices {
    keyboards: HashMap<PathBuf, Keyboard>,
    warned: HashSet<PathBuf>,
}

impl Devices {
    fn modifiers(&self) -> Modifiers {
        self.keyboards
            .values()
            .fold(Modifiers::default(), |flags, keyboard| {
                flags.union(keyboard.filter.modifiers())
            })
    }

    fn keypress(&mut self, path: &Path, event: InputEvent) -> Option<KeyChord> {
        let keyboard = self.keyboards.get_mut(path)?;
        let resync = keyboard.filter.dropped
            && matches!(
                event.destructure(),
                EventSummary::Synchronization(_, SynchronizationCode::SYN_REPORT, _)
            );
        let key = keyboard.filter.keypress(event);
        if resync {
            match keyboard.device.get_key_state() {
                Ok(keys) => keyboard.filter.resync(keys.iter()),
                Err(error) => eprintln!("cannot resync keyboard {}: {error}", path.display()),
            }
        }
        key.map(|key| KeyChord {
            key,
            modifiers: self.modifiers(),
        })
    }

    fn rescan(&mut self) -> io::Result<()> {
        let paths: HashSet<_> = event_paths()?.into_iter().collect();
        self.keyboards.retain(|path, _| paths.contains(path));
        self.warned.retain(|path| paths.contains(path));
        for path in paths {
            if self.keyboards.contains_key(&path) {
                continue;
            }
            match open_keyboard(&path) {
                Ok(Some(keyboard)) => {
                    eprintln!(
                        "keyboard: {} ({})",
                        path.display(),
                        keyboard.device.name().unwrap_or("unnamed")
                    );
                    self.keyboards.insert(path.clone(), keyboard);
                    self.warned.remove(&path);
                }
                Ok(None) => {
                    self.warned.remove(&path);
                }
                Err(error) => {
                    if self.warned.insert(path.clone()) {
                        eprintln!("cannot read {}: {error}", path.display());
                    }
                }
            }
        }
        Ok(())
    }
}

pub struct InputReader {
    stop: Arc<AtomicBool>,
    sender: Sender<KeyChord>,
    thread: Option<JoinHandle<()>>,
}

impl InputReader {
    pub fn start() -> Result<(Self, Receiver<KeyChord>)> {
        let mut devices = Devices::default();
        devices.rescan().context("cannot enumerate /dev/input")?;
        ensure!(
            !devices.keyboards.is_empty(),
            "no readable keyboard in /dev/input; grant your user read access to keyboard event devices (see README.md), or use --demo"
        );
        let (sender, receiver) = async_channel::bounded(1024);
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let worker_sender = sender.clone();
        let thread = thread::Builder::new()
            .name("evdev-reader".into())
            .spawn(move || read_loop(devices, worker_sender, worker_stop))?;
        Ok((
            Self {
                stop,
                sender,
                thread: Some(thread),
            },
            receiver,
        ))
    }
}

impl Drop for InputReader {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.sender.close(); // Unblock a sender even when its queue is full.
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn read_loop(mut devices: Devices, sender: Sender<KeyChord>, stop: Arc<AtomicBool>) {
    let mut scanned = Instant::now();
    while !stop.load(Ordering::Relaxed) && !sender.is_closed() {
        if scanned.elapsed() >= RESCAN_INTERVAL {
            if let Err(error) = devices.rescan() {
                eprintln!("input rescan failed: {error}");
            }
            scanned = Instant::now();
        }
        let ready = {
            let paths: Vec<_> = devices.keyboards.keys().cloned().collect();
            let mut fds: Vec<_> = paths
                .iter()
                .map(|path| PollFd::new(devices.keyboards[path].device.as_fd(), PollFlags::POLLIN))
                .collect();
            match poll(&mut fds, 500u16) {
                Ok(_) => paths
                    .into_iter()
                    .zip(fds.iter())
                    .filter_map(|(path, fd)| {
                        let flags = fd.revents().unwrap_or(PollFlags::empty());
                        (!flags.is_empty()).then_some((path, flags))
                    })
                    .collect::<Vec<_>>(),
                Err(nix::errno::Errno::EINTR) => continue,
                Err(error) => {
                    eprintln!("input poll failed: {error}");
                    break;
                }
            }
        };
        let mut pending = Vec::new();
        for (path, flags) in ready {
            if flags.intersects(PollFlags::POLLERR | PollFlags::POLLHUP | PollFlags::POLLNVAL) {
                devices.keyboards.remove(&path);
                continue;
            }
            let keyboard = devices
                .keyboards
                .get_mut(&path)
                .expect("polled keyboard exists");
            let fetched = keyboard
                .device
                .fetch_events()
                .map(|events| events.collect::<Vec<_>>());
            let events = match fetched {
                Ok(events) => events,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => continue,
                Err(error) => {
                    eprintln!("keyboard disconnected {}: {error}", path.display());
                    devices.keyboards.remove(&path);
                    continue;
                }
            };
            pending.extend(events.into_iter().map(|event| (path.clone(), event)));
        }
        // Preserve ordering when a modifier and the ordinary key come from different keyboards.
        pending.sort_by_key(|(_, event)| event.timestamp());
        for (path, event) in pending {
            if let Some(chord) = devices.keypress(&path, event)
                && sender.send_blocking(chord).is_err()
            {
                return;
            }
        }
    }
    sender.close();
}

pub fn list_devices() -> Result<()> {
    for path in event_paths().context("cannot enumerate /dev/input")? {
        match open_keyboard(&path) {
            Ok(Some(keyboard)) => println!(
                "{}\t{}",
                path.display(),
                keyboard.device.name().unwrap_or("unnamed")
            ),
            Ok(None) => {}
            Err(error) => eprintln!("{}\t{error}", path.display()),
        }
    }
    Ok(())
}
