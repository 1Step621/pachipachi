mod animation;
mod config;
mod images;
mod input;
mod keys;
mod pet;

use std::{env, path::PathBuf};

use anyhow::{Result, ensure};
use clap::Parser;

#[derive(Parser)]
#[command(version, about = "A typing pet overlay for Linux Wayland")]
struct Args {
    /// Config file (default: $XDG_CONFIG_HOME/pachipachi/config.toml)
    #[arg(short, long)]
    config: Option<PathBuf>,
    /// Validate config and decode all images without opening an overlay
    #[arg(long, conflicts_with_all = ["demo", "list_devices"])]
    check: bool,
    /// Animate simulated keypresses without accessing /dev/input
    #[arg(long, conflicts_with = "list_devices")]
    demo: bool,
    /// List readable keyboards and input-device permission errors
    #[arg(long)]
    list_devices: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    if args.list_devices {
        return input::list_devices();
    }
    let path = match args.config {
        Some(path) => path,
        None => config::default_path()?,
    };
    let config = config::Config::load(&path)?;
    let images = images::PetImages::load(&config.images)?;
    if args.check {
        println!("config and images OK: {}", path.display());
        return Ok(());
    }
    ensure!(
        env::var_os("WAYLAND_DISPLAY").is_some_and(|display| !display.is_empty()),
        "WAYLAND_DISPLAY is not set; run pachipachi in a Wayland session"
    );
    let (_reader, receiver) = if args.demo {
        (None, None)
    } else {
        let (reader, receiver) = input::InputReader::start()?;
        (Some(reader), Some(receiver))
    };
    pet::run(config, images, receiver)
}
