//! The desktop's light or dark preference and its reduced-motion preference.
//!
//! There is nothing in the Wayland protocol about either, and that is not an oversight: a
//! compositor composites, and what an application should look like is a setting of the desktop.
//! The settings live on the session bus, behind the portal every sandboxed application already
//! reaches its desktop through, and they are read there —
//! which is also the only way they work from inside a sandbox, where the toolkit's own
//! configuration files are not reachable at all.
//!
//! They are read and watched on a thread of its own. The bus is a socket like any other, an answer
//! from it takes as long as the portal takes, and the loop's own thread waits on nothing.

use std::sync::{Arc, Mutex};

use zgui_platform::{ColorScheme, WakeReason, Waker};

/// Where the setting lives.
const PORTAL: &str = "org.freedesktop.portal.Desktop";
/// The object it lives on.
const PATH: &str = "/org/freedesktop/portal/desktop";
/// The interface that reads it.
const SETTINGS: &str = "org.freedesktop.portal.Settings";
/// The namespace the appearance settings are grouped under.
const APPEARANCE: &str = "org.freedesktop.appearance";
/// The light-or-dark setting.
const SCHEME: &str = "color-scheme";
/// The reduced-motion setting.
const MOTION: &str = "reduced-motion";

/// The preferences as the portal last reported them, shared with the loop.
#[derive(Clone, Default)]
pub struct Settled {
    /// The light or dark preference, where it could be discovered.
    pub scheme: Arc<Mutex<Option<ColorScheme>>>,
    /// Whether the desktop asks for reduced motion, where it could be discovered.
    pub reduced_motion: Arc<Mutex<Option<bool>>>,
}

/// The preference a portal value stands for.
///
/// Three values and only two of them are a preference: zero means *no preference*, which is not
/// the same as light. A desktop that has not been asked must not show every person who chose dark
/// a white flash, so it answers with nothing and the framework's own default decides.
pub const fn scheme(value: u32) -> Option<ColorScheme> {
    match value {
        1 => Some(ColorScheme::Dark),
        2 => Some(ColorScheme::Light),
        _ => None,
    }
}

/// The reduced-motion answer a portal value stands for.
///
/// Zero states no preference and one asks for reduced motion. A value the portal does not define
/// is unknown.
pub const fn reduced_motion(value: u32) -> Option<bool> {
    match value {
        0 => Some(false),
        1 => Some(true),
        _ => None,
    }
}

/// Reads the preferences and watches them, answering through `waker`.
///
/// Answers with whether a thread was started at all. A session with no portal — a bare compositor,
/// a machine with no bus — is not a failure: the preferences are unknown, which the contract
/// already has an answer for.
pub fn watch(waker: Arc<dyn Waker>, settled: Settled) -> bool {
    let started = std::thread::Builder::new()
        .name("zgui-appearance".to_owned())
        .spawn(move || follow(&waker, &settled));
    started.is_ok()
}

/// Reads the settings, then stays on the bus reporting every change to them.
fn follow(waker: &Arc<dyn Waker>, settled: &Settled) {
    let Ok(connection) = zbus::blocking::Connection::session() else {
        tracing::debug!("no session bus, so this desktop's appearance preferences are unknown");
        return;
    };
    let proxy = zbus::blocking::Proxy::new(&connection, PORTAL, PATH, SETTINGS);
    let Ok(proxy) = proxy else {
        return;
    };

    if let Some(found) = read(&proxy, SCHEME).and_then(scheme) {
        publish_scheme(waker, settled, found);
    }
    if let Some(found) = read(&proxy, MOTION).and_then(reduced_motion) {
        publish_motion(waker, settled, found);
    }

    // The portal reports a change as a signal naming the namespace and the key, so everything else
    // on the interface is ignored rather than filtered by the bus — the traffic is a handful of
    // messages a session.
    let Ok(signals) = proxy.receive_signal("SettingChanged") else {
        return;
    };
    for message in signals {
        let body = message.body();
        let Ok((namespace, key, value)) =
            body.deserialize::<(String, String, zbus::zvariant::OwnedValue)>()
        else {
            continue;
        };
        if namespace != APPEARANCE {
            continue;
        }
        let found = unwrap_twice(value);
        if key == SCHEME {
            if let Some(found) = found.and_then(scheme) {
                publish_scheme(waker, settled, found);
            }
        } else if key == MOTION
            && let Some(found) = found.and_then(reduced_motion)
        {
            publish_motion(waker, settled, found);
        }
    }
}

/// Asks the portal what the appearance setting `key` is now.
fn read(proxy: &zbus::blocking::Proxy<'_>, key: &str) -> Option<u32> {
    // `ReadOne` is the newer call and answers with the value itself; `Read` is what older portals
    // have and wraps it twice. Both are tried, because a desktop that only has the second is a
    // desktop this would otherwise report as having no preference at all.
    let one: Option<zbus::zvariant::OwnedValue> = proxy.call("ReadOne", &(APPEARANCE, key)).ok();
    if let Some(found) = one.and_then(|value| u32::try_from(value).ok()) {
        return Some(found);
    }
    let nested: zbus::zvariant::OwnedValue = proxy.call("Read", &(APPEARANCE, key)).ok()?;
    unwrap_twice(nested)
}

/// The number inside a value the older call wrapped in two layers of variant.
fn unwrap_twice(value: zbus::zvariant::OwnedValue) -> Option<u32> {
    if let Ok(found) = u32::try_from(&value) {
        return Some(found);
    }
    let zbus::zvariant::Value::Value(inner) = zbus::zvariant::Value::from(value) else {
        return None;
    };
    u32::try_from(*inner).ok()
}

/// Records a light or dark preference and tells the loop, when it is a change.
fn publish_scheme(waker: &Arc<dyn Waker>, settled: &Settled, found: ColorScheme) {
    if store(&settled.scheme, found) {
        waker.wake(WakeReason::ColorSchemeChanged);
    }
}

/// Records a reduced-motion preference and tells the loop, when it is a change.
fn publish_motion(waker: &Arc<dyn Waker>, settled: &Settled, found: bool) {
    if store(&settled.reduced_motion, found) {
        waker.wake(WakeReason::ReducedMotionChanged);
    }
}

/// Stores `found` in `held`, and reports whether that changed it.
fn store<T: PartialEq>(held: &Mutex<Option<T>>, found: T) -> bool {
    let mut held = held.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if held.as_ref() == Some(&found) {
        return false;
    }
    *held = Some(found);
    true
}

#[cfg(test)]
mod tests {
    use super::{reduced_motion, scheme, store};
    use std::sync::Mutex;
    use zgui_platform::ColorScheme;

    #[test]
    fn no_preference_is_not_light() {
        // A desktop that has not been asked must not show everyone who chose dark a white flash.
        assert_eq!(scheme(0), None);
        assert_eq!(scheme(7), None);
    }

    #[test]
    fn the_two_real_answers_are_the_two_the_portal_numbers() {
        assert_eq!(scheme(1), Some(ColorScheme::Dark));
        assert_eq!(scheme(2), Some(ColorScheme::Light));
    }

    #[test]
    fn the_reduced_motion_values_are_the_two_the_portal_numbers() {
        assert_eq!(reduced_motion(0), Some(false));
        assert_eq!(reduced_motion(1), Some(true));
        assert_eq!(reduced_motion(2), None);
    }

    #[test]
    fn a_repeated_answer_is_not_a_change() {
        let held = Mutex::new(None);
        assert!(store(&held, true));
        assert!(!store(&held, true));
        assert!(store(&held, false));
    }
}
