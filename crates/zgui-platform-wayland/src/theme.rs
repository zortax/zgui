//! The desktop's light or dark preference.
//!
//! There is nothing in the Wayland protocol about it, and that is not an oversight: a compositor
//! composites, and what an application should look like is a setting of the desktop rather than of
//! the display server. The setting lives on the session bus, behind the portal every sandboxed
//! application already reaches its desktop through, and it is read there — which is also the only
//! way it works from inside a sandbox, where the toolkit's own configuration files are not
//! reachable at all.
//!
//! It is read and watched on a thread of its own. The bus is a socket like any other, an answer
//! from it takes as long as the portal takes, and the loop's own thread waits on nothing.

use std::sync::Arc;

use zgui_platform::{ColorScheme, WakeReason, Waker};

/// Where the setting lives.
const PORTAL: &str = "org.freedesktop.portal.Desktop";
/// The object it lives on.
const PATH: &str = "/org/freedesktop/portal/desktop";
/// The interface that reads it.
const SETTINGS: &str = "org.freedesktop.portal.Settings";
/// The namespace the appearance settings are grouped under.
const APPEARANCE: &str = "org.freedesktop.appearance";
/// The setting itself.
const SCHEME: &str = "color-scheme";

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

/// Reads the preference and watches it, answering through `waker`.
///
/// Answers with whether a thread was started at all. A session with no portal — a bare compositor,
/// a machine with no bus — is not a failure: the preference is unknown, which the contract already
/// has an answer for.
pub fn watch(waker: Arc<dyn Waker>, settled: Arc<std::sync::Mutex<Option<ColorScheme>>>) -> bool {
    let started = std::thread::Builder::new()
        .name("zgui-appearance".to_owned())
        .spawn(move || follow(&waker, &settled));
    started.is_ok()
}

/// Reads the setting, then stays on the bus reporting every change to it.
fn follow(waker: &Arc<dyn Waker>, settled: &Arc<std::sync::Mutex<Option<ColorScheme>>>) {
    let Ok(connection) = zbus::blocking::Connection::session() else {
        tracing::debug!("no session bus, so this desktop's light or dark preference is unknown");
        return;
    };
    let proxy = zbus::blocking::Proxy::new(&connection, PORTAL, PATH, SETTINGS);
    let Ok(proxy) = proxy else {
        return;
    };

    if let Some(found) = read(&proxy) {
        publish(waker, settled, found);
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
        if namespace != APPEARANCE || key != SCHEME {
            continue;
        }
        if let Some(found) = unwrap_twice(value).and_then(scheme) {
            publish(waker, settled, found);
        }
    }
}

/// Asks the portal what the preference is now.
fn read(proxy: &zbus::blocking::Proxy<'_>) -> Option<ColorScheme> {
    // `ReadOne` is the newer call and answers with the value itself; `Read` is what older portals
    // have and wraps it twice. Both are tried, because a desktop that only has the second is a
    // desktop this would otherwise report as having no preference at all.
    let one: Option<zbus::zvariant::OwnedValue> = proxy.call("ReadOne", &(APPEARANCE, SCHEME)).ok();
    if let Some(found) = one
        .and_then(|value| u32::try_from(value).ok())
        .and_then(scheme)
    {
        return Some(found);
    }
    let nested: zbus::zvariant::OwnedValue = proxy.call("Read", &(APPEARANCE, SCHEME)).ok()?;
    unwrap_twice(nested).and_then(scheme)
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

/// Records a preference and tells the loop, when it is a change.
fn publish(
    waker: &Arc<dyn Waker>,
    settled: &Arc<std::sync::Mutex<Option<ColorScheme>>>,
    found: ColorScheme,
) {
    let mut held = settled
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if *held == Some(found) {
        return;
    }
    *held = Some(found);
    drop(held);
    waker.wake(WakeReason::ColorSchemeChanged);
}

#[cfg(test)]
mod tests {
    use super::scheme;
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
}
