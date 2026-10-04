use std::{
    cell::RefCell,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Result;
use async_channel::Receiver;
use evdev::KeyCode;
use gpui::{
    App, AppContext, Bounds, Context, IntoElement, ObjectFit, Render, RenderImage, Task, Window,
    WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions, div, img,
    layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions},
    point,
    prelude::*,
    px, size,
};

use crate::{
    animation::{BOUNCE_HEIGHT, NEUTRAL_DURATION, TypingState, VISIBLE_DURATION},
    config::{Config, Corner},
    images::PetImages,
    keys::KeyChord,
};

struct Pet {
    images: PetImages,
    current: Arc<RenderImage>,
    state: TypingState,
    width: f32,
    height: f32,
    hide_task: Option<Task<()>>,
}

impl Pet {
    fn new(
        images: PetImages,
        width: f32,
        height: f32,
        receiver: Option<Receiver<KeyChord>>,
        cx: &mut Context<Self>,
    ) -> Self {
        let current = images.neutral();
        cx.spawn(async move |this, cx| {
            if let Some(receiver) = receiver {
                while let Ok(key) = receiver.recv().await {
                    if this.update(cx, |pet, cx| pet.press(key, cx)).is_err() {
                        break;
                    }
                }
            } else {
                let sequence = [
                    KeyChord::unmodified(KeyCode::KEY_A),
                    KeyChord::unmodified(KeyCode::KEY_B),
                    KeyChord::unmodified(KeyCode::KEY_ENTER),
                    KeyChord::unmodified(KeyCode::KEY_SPACE),
                    "Shift+KEY_1".parse().expect("valid demo chord"),
                    KeyChord::unmodified(KeyCode::KEY_HENKAN),
                    KeyChord::unmodified(KeyCode::KEY_ESC),
                    KeyChord::unmodified(KeyCode::KEY_F1),
                    "Ctrl+KEY_C".parse().expect("valid demo chord"),
                ];
                let mut index = 0;
                loop {
                    // Pause between sequences so demo mode also shows the idle timeout.
                    let delay = if index > 0 && index % sequence.len() == 0 {
                        Duration::from_secs(3)
                    } else {
                        Duration::from_millis(600)
                    };
                    cx.background_executor().timer(delay).await;
                    let key = sequence[index % sequence.len()];
                    index += 1;
                    if this.update(cx, |pet, cx| pet.press(key, cx)).is_err() {
                        break;
                    }
                }
            }
        })
        .detach();
        Self {
            images,
            current,
            state: TypingState::default(),
            width,
            height,
            hide_task: None,
        }
    }

    fn press(&mut self, key: KeyChord, cx: &mut Context<Self>) {
        if !self.images.reacts_to(key) {
            return;
        }
        let now = Instant::now();
        self.state.press(now);
        self.current = self.images.select(Some(key), self.state.odd);
        // Cancelling the previous task prevents stale neutral/fade timers after a new press.
        self.hide_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer((now + NEUTRAL_DURATION).saturating_duration_since(Instant::now()))
                .await;
            if this.update(cx, |_, cx| cx.notify()).is_err() {
                return;
            }
            cx.background_executor()
                .timer((now + VISIBLE_DURATION).saturating_duration_since(Instant::now()))
                .await;
            let _ = this.update(cx, |_, cx| cx.notify());
        }));
        cx.notify();
    }
}

impl Render for Pet {
    fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let now = Instant::now();
        if self.state.is_animating(now) {
            window.request_animation_frame();
        }
        let current = if self.state.is_idle(now) {
            self.images.neutral()
        } else {
            self.current.clone()
        };
        div()
            .relative()
            .size_full()
            .when(self.state.is_visible(now), |root| {
                root.child(
                    img(current)
                        .absolute()
                        .left(px(0.0))
                        .top(px(BOUNCE_HEIGHT - self.state.height(now)))
                        .w(px(self.width))
                        .h(px(self.height))
                        .opacity(self.state.opacity(now))
                        .object_fit(ObjectFit::Contain),
                )
            })
    }
}

pub fn run(config: Config, images: PetImages, receiver: Option<Receiver<KeyChord>>) -> Result<()> {
    let failure = Rc::new(RefCell::new(None));
    let startup_failure = failure.clone();
    gpui_platform::application().run(move |cx: &mut App| {
        let position = &config.position;
        let (anchor, margins) = match position.anchor {
            Corner::TopLeft => (Anchor::TOP | Anchor::LEFT, (position.y, 0, 0, position.x)),
            Corner::TopRight => (Anchor::TOP | Anchor::RIGHT, (position.y, position.x, 0, 0)),
            Corner::BottomLeft => (Anchor::BOTTOM | Anchor::LEFT, (0, 0, position.y, position.x)),
            Corner::BottomRight => (Anchor::BOTTOM | Anchor::RIGHT, (0, position.x, position.y, 0)),
        };
        let width = f32::from(config.images.width);
        let height = f32::from(config.images.height);
        let result = cx.open_window(WindowOptions {
            titlebar: None,
            focus: false,
            is_movable: false,
            is_resizable: false,
            is_minimizable: false,
            inactive_frame_interval: None,
            app_id: Some("pachipachi".into()),
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(point(px(0.0), px(0.0)), size(px(width), px(height + BOUNCE_HEIGHT))))),
            window_background: WindowBackgroundAppearance::Transparent,
            kind: WindowKind::LayerShell(LayerShellOptions {
                namespace: "pachipachi".into(),
                layer: Layer::Overlay,
                anchor,
                exclusive_zone: Some(px(-1.0)),
                margin: Some((px(f32::from(margins.0)), px(f32::from(margins.1)), px(f32::from(margins.2)), px(f32::from(margins.3)))),
                keyboard_interactivity: KeyboardInteractivity::None,
                ..Default::default()
            }),
            ..Default::default()
        }, move |window, cx| {
            window.set_input_region(Some(&[]));
            cx.new(|cx| Pet::new(images, width, height, receiver, cx))
        });
        if let Err(error) = result {
            *startup_failure.borrow_mut() = Some(error.context("cannot create overlay; a Wayland compositor with wlr-layer-shell support is required"));
            cx.quit();
        }
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() { cx.quit(); }
        }).detach();
    });
    if let Some(error) = failure.borrow_mut().take() {
        return Err(error);
    }
    Ok(())
}
