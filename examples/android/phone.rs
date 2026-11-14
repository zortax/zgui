//! Device features: multi-touch, photo picker, share sheet and links.
use crate::lab::{Haptic, State, haptic};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use zgui::prelude::*;
use zgui_ui::prelude::*;

/// Offset in CSS pixels, scale factor and rotation in degrees.
#[derive(Clone, Copy, PartialEq)]
struct Pose {
    x: f32,
    y: f32,
    scale: f32,
    angle: f32,
}

const REST: Pose = Pose {
    x: 0.0,
    y: 0.0,
    scale: 1.0,
    angle: 0.0,
};

/// Contacts on the pad, and the pose and contact geometry when the set last changed.
#[derive(Default)]
struct Contacts {
    points: HashMap<u64, (f32, f32)>,
    start: Option<(Pose, Geometry)>,
}

/// Midpoint, spread and angle of up to two contacts.
#[derive(Clone, Copy)]
struct Geometry {
    mid: (f32, f32),
    spread: f32,
    angle: f32,
}

impl Contacts {
    fn geometry(&self) -> Option<Geometry> {
        let mut points = self.points.values();
        let a = *points.next()?;
        let Some(&b) = points.next() else {
            return Some(Geometry {
                mid: a,
                spread: 0.0,
                angle: 0.0,
            });
        };
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        Some(Geometry {
            mid: ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0),
            spread: dx.hypot(dy),
            angle: dy.atan2(dx).to_degrees(),
        })
    }
}

/// Drag with one finger, pinch and rotate with two, long-press to reset.
#[component]
fn GesturePad() -> impl IntoView {
    let s = State::current();
    let pose = RwSignal::new_local(REST);
    let contacts = Rc::new(RefCell::new(Contacts::default()));
    // The contact set changed: the current pose becomes the base for the new contacts.
    let rebase = move |contacts: &mut Contacts| {
        let geometry = contacts.geometry();
        contacts.start = geometry.map(|geometry| (pose.get_untracked(), geometry));
    };
    let down = {
        let contacts = Rc::clone(&contacts);
        move |ev: &mut EventCx<'_, events::PointerDown>| {
            ev.capture_pointer();
            let mut contacts = contacts.borrow_mut();
            contacts
                .points
                .insert(ev.id.raw(), (ev.position.x.0, ev.position.y.0));
            rebase(&mut contacts);
        }
    };
    let moved = {
        let contacts = Rc::clone(&contacts);
        move |ev: &mut EventCx<'_, events::PointerMove>| {
            let mut contacts = contacts.borrow_mut();
            let Some(point) = contacts.points.get_mut(&ev.id.raw()) else {
                return;
            };
            *point = (ev.position.x.0, ev.position.y.0);
            let (Some((base, from)), Some(now)) = (contacts.start, contacts.geometry()) else {
                return;
            };
            let two = contacts.points.len() > 1;
            pose.set(Pose {
                x: base.x + now.mid.0 - from.mid.0,
                y: base.y + now.mid.1 - from.mid.1,
                scale: if two && from.spread > 0.0 {
                    (base.scale * now.spread / from.spread).clamp(0.4, 4.0)
                } else {
                    base.scale
                },
                angle: if two {
                    base.angle + now.angle - from.angle
                } else {
                    base.angle
                },
            });
        }
    };
    let up = {
        let contacts = Rc::clone(&contacts);
        move |id: u64| {
            let mut contacts = contacts.borrow_mut();
            contacts.points.remove(&id);
            rebase(&mut contacts);
        }
    };
    let up_cancel = up.clone();
    let reset = {
        let contacts = Rc::clone(&contacts);
        move || {
            pose.set(REST);
            rebase(&mut contacts.borrow_mut());
        }
    };
    let transform = move || {
        let p = pose.get();
        Some(format!(
            "translate({}px, {}px) rotate({}deg) scale({})",
            p.x, p.y, p.angle, p.scale
        ))
    };
    view! {
        column(
            class = "pad",
            a11y:label = "Gesture pad",
            on:pointer_down = down,
            on:pointer_move = moved,
            on:pointer_up = move |ev| { ev.release_pointer(); up(ev.id.raw()) },
            on:pointer_cancel = move |ev| { ev.release_pointer(); up_cancel(ev.id.raw()) },
            on:context_menu = move |ev| {
                ev.prevent_default();
                haptic(Haptic::LongPress);
                reset();
            }
        ) {
            column(class = "pad-item", style:transform = transform) {
                if move || s.photo.with(Option::is_some) {
                    image(class = "pad-photo", src = move || s.photo.get())
                } else {
                    column(class = "pad-tile") { text {"Z"} }
                }
            }
        }
        text(class = "muted") {{move || {
            let p = pose.get();
            format!("{:.0}% • {:.0}° • long-press to reset", p.scale * 100.0, p.angle)
        }}}
    }
}

#[component]
pub(crate) fn Phone() -> impl IntoView {
    let s = State::current();
    let report = move || {
        let done = s
            .tasks
            .with(|tasks| tasks.iter().filter(|t| t.done).count());
        format!(
            "{} finished {done} of {} checks in ZGUI Mobile Lab.",
            s.name.get_untracked(),
            s.tasks.with_untracked(Vec::len)
        )
    };
    let pick = move |_: &mut EventCx<'_, events::Click>| {
        #[cfg(target_os = "android")]
        {
            let ui = ui();
            zgui::app::android::pick_image(move |path| {
                ui.post(move || {
                    if let Some(path) = path {
                        haptic(Haptic::Confirm);
                        s.photo.set(path.to_str().map(Into::into));
                    }
                });
            });
        }
    };
    view! {
        column(class = "stack") {
            WorkProbe()
            column(class = "card") {
                text(class = "section-title") {"Touch"}
                text(class = "muted") {"Drag with one finger. Pinch and twist with two."}
                GesturePad()
            }
            column(class = "card") {
                text(class = "section-title") {"Photo"}
                text(class = "muted") {"System photo picker. The photo appears on the touch pad."}
                row(class = "actions") {
                    Button(on:click = pick) {"Pick photo"}
                    Button(variant = ButtonVariant::Outline, disabled = Signal::derive_local(move || s.photo.with(Option::is_none)), on:click = move |_| s.photo.set(None)) {"Clear"}
                }
            }
            column(class = "card") {
                text(class = "section-title") {"Share & links"}
                text(class = "muted") {{move || {
                    s.tasks.track();
                    report()
                }}}
                row(class = "actions") {
                    Button(on:click = move |_| {
                        #[cfg(target_os = "android")]
                        zgui::app::android::share(&report());
                    }) {"Share report"}
                    Button(variant = ButtonVariant::Outline, on:click = move |_| {
                        #[cfg(target_os = "android")]
                        zgui::app::android::open_url("https://github.com/zortax/zgui");
                    }) {"Open GitHub"}
                }
            }
        }
    }
}

/// Real worker jobs awaited by UI task; controls remain live while jobs run.
#[component]
fn WorkProbe() -> impl IntoView {
    let status = RwSignal::new_local("Ready".to_owned());
    let running = RwSignal::new_local(false);
    let taps = RwSignal::new_local(0_u32);
    let start = move |_: &mut EventCx<'_, events::Click>| {
        if running.get_untracked() {
            return;
        }
        running.set(true);
        status.set("Working for 2 seconds; tap responsiveness check".into());
        zgui::reactive::spawn_local(async move {
            let ui_thread = std::thread::current().id();
            let async_job =
                zgui::reactive::background(async { format!("{:?}", std::thread::current().id()) });
            let first = zgui::reactive::blocking(|| {
                std::thread::sleep(std::time::Duration::from_secs(2));
                format!("{:?}", std::thread::current().id())
            });
            let second = zgui::reactive::blocking(|| {
                std::thread::sleep(std::time::Duration::from_secs(2));
                format!("{:?}", std::thread::current().id())
            });
            let async_thread = async_job.await;
            let first = first.await;
            let second = second.await;
            status.set(format!(
                "Done. UI {ui_thread:?}; async {async_thread}; workers {first}, {second}"
            ));
            running.set(false);
        });
    };
    view! {
        column(class = "card") {
            text(class = "section-title") {"Async & workers"}
            text(class = "muted") {"UI task awaits async worker and two blocking jobs. Home does not make jobs durable."}
            text {{move || status.get()}}
            row(class = "actions") {
                Button(disabled = Signal::derive_local(move || running.get()), on:click = start) {"Run workers"}
                Button(variant = ButtonVariant::Outline, on:click = move |_| taps.update(|n| *n += 1)) {
                    {move || format!("Responsive: {}", taps.get())}
                }
            }
        }
    }
}
