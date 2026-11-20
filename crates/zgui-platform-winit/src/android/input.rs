//! Input that `ZguiActivity` keeps in Java view dispatch, so Android accessibility receives
//! hover exploration first.
use jni::objects::{JClass, JString};
use jni::{EnvUnowned, jni_str};
use zgui_geom::{CssPx, Point};
use zgui_platform::SurfaceEvent;
use zgui_vocab::{
    Key, KeyEvent, KeyLocation, KeyState, Modifiers, NamedKey, PhysicalKey, PointerAction,
    PointerButton, PointerEvent, PointerId, PointerKind, ScrollDelta, ScrollPhase, Timestamp,
    WheelEvent,
};

fn modifiers(bits: i32) -> Modifiers {
    Modifiers::NONE
        .with(Modifiers::SHIFT, bits & 1 != 0)
        .with(Modifiers::CONTROL, bits & 2 != 0)
        .with(Modifiers::ALT, bits & 4 != 0)
        .with(Modifiers::META, bits & 8 != 0)
}

// SAFETY: signature matches Zgui.pointer.
extern "system" fn pointer<'a>(
    _: EnvUnowned<'a>,
    _: JClass<'a>,
    action: i32,
    id: i32,
    x: f32,
    y: f32,
    pressure: f32,
    tool: i32,
    bits: i32,
    button: i32,
) {
    let action = match action {
        0 => PointerAction::Pressed,
        1 => PointerAction::Released,
        2 => PointerAction::Moved,
        3 => PointerAction::Cancelled,
        9 => PointerAction::Entered,
        10 => PointerAction::Left,
        _ => return,
    };
    let kind = match tool {
        2 | 4 => PointerKind::Pen,
        3 => PointerKind::Mouse,
        _ => PointerKind::Touch,
    };
    crate::android_host::dispatch_input(SurfaceEvent::Pointer {
        action,
        event: PointerEvent {
            id: if kind == PointerKind::Mouse {
                PointerId::MOUSE
            } else {
                PointerId::new(id as u64 + 1)
            },
            kind,
            primary: id == 0,
            position: Point::new(CssPx(x), CssPx(y)),
            button: match button {
                0 => None,
                2 => Some(PointerButton::Secondary),
                4 => Some(PointerButton::Middle),
                _ => Some(PointerButton::Primary),
            },
            pressure: Some(pressure),
        },
        modifiers: modifiers(bits),
        timestamp: Timestamp::ORIGIN,
    });
}
fn read_key(text: String) -> Key {
    if let Some(name) = text.strip_prefix("named:") {
        name.parse::<NamedKey>()
            .map_or_else(|_| Key::Other(name.into()), Key::Named)
    } else {
        Key::Character(text.into())
    }
}
// SAFETY: signature matches Zgui.key.
extern "system" fn key<'a>(
    mut env: EnvUnowned<'a>,
    _: JClass<'a>,
    down: bool,
    text: JString<'a>,
    plain: JString<'a>,
    scan: i32,
    bits: i32,
    repeat: bool,
) {
    env.with_env(|_| -> jni::errors::Result<()> {
        let key = read_key(text.to_string());
        crate::android_host::dispatch_input(SurfaceEvent::Key {
            state: if down {
                KeyState::Pressed
            } else {
                KeyState::Released
            },
            event: KeyEvent {
                key,
                key_without_modifiers: read_key(plain.to_string()),
                physical: PhysicalKey::Unidentified(scan as u32),
                location: KeyLocation::Standard,
                repeat,
            },
            modifiers: modifiers(bits),
            timestamp: Timestamp::ORIGIN,
        });
        Ok(())
    })
    .resolve::<jni::errors::LogErrorAndDefault>();
}
// SAFETY: signature matches Zgui.wheel.
extern "system" fn wheel<'a>(
    _: EnvUnowned<'a>,
    _: JClass<'a>,
    x: f32,
    y: f32,
    dx: f32,
    dy: f32,
    bits: i32,
) {
    crate::android_host::dispatch_input(SurfaceEvent::Wheel {
        event: WheelEvent {
            delta: ScrollDelta::Lines { x: -dx, y: -dy },
            phase: ScrollPhase::Discrete,
            position: Point::new(CssPx(x), CssPx(y)),
            id: PointerId::MOUSE,
            kind: PointerKind::Mouse,
        },
        modifiers: modifiers(bits),
        timestamp: Timestamp::ORIGIN,
    });
}

pub(super) fn methods() -> Vec<jni::NativeMethod<'static>> {
    // SAFETY: signatures match the static native declarations in Zgui.java.
    #[allow(unsafe_code)]
    unsafe {
        vec![
            jni::NativeMethod::from_raw_parts(
                jni_str!("pointer"),
                jni_str!("(IIFFFIII)V"),
                pointer as *mut core::ffi::c_void,
            ),
            jni::NativeMethod::from_raw_parts(
                jni_str!("key"),
                jni_str!("(ZLjava/lang/String;Ljava/lang/String;IIZ)V"),
                key as *mut core::ffi::c_void,
            ),
            jni::NativeMethod::from_raw_parts(
                jni_str!("wheel"),
                jni_str!("(FFFFI)V"),
                wheel as *mut core::ffi::c_void,
            ),
        ]
    }
}
