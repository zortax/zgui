//! The session lock, as the application holds it.
//!
//! A lock is asked for on the next turn, confirmed or ended by the desktop, and unlocked only
//! after the confirmation. Its lock windows open on the outputs they name, one per output, and
//! close when the lock is over.

mod support;

use zgui_geom::{DevicePx, Point, Size};
use zgui_platform::{
    CloseReason, MonitorId, MonitorInfo, MonitorTarget, PlatformCapabilities, PlatformCx,
    SurfaceRole,
};
use zgui_platform_headless::{Harness, Headless, SessionLockState};
use zgui_reactive::prelude::*;
use zgui_runtime::lock::{LockError, LockHandle, SessionLock};
use zgui_runtime::{App, ExitPolicy, Runtime};

/// An output numbered `id`.
fn output(id: u64) -> MonitorInfo {
    MonitorInfo::new(
        Point::new(DevicePx(0.0), DevicePx(0.0)),
        Size::new(DevicePx(1920.0), DevicePx(1080.0)),
        1.0,
    )
    .with_id(MonitorId::new(id))
}

/// An application with no window, over a platform with two outputs that can lock.
fn application(lockable: bool) -> Harness<Runtime> {
    let handler = App::new()
        .without_primary_window()
        .with_exit_policy(ExitPolicy::Explicit)
        .with_renderer(Box::new(support::capture))
        .into_handler(|_cx: &mut zgui_view::BuildCx<'_>| unreachable!("there is no primary window"))
        .expect("the reactive runtime installs");
    let mut capabilities = PlatformCapabilities::none();
    capabilities.session_lock = lockable;
    let platform = Headless::new()
        .with_capabilities(capabilities)
        .with_monitors(vec![output(1), output(2)]);
    let mut harness = Harness::over(platform, handler);
    harness.settle(4);
    harness
}

/// Asks for a lock and opens a lock window on each output.
fn lock(harness: &mut Harness<Runtime>) -> LockHandle {
    let lock = harness
        .app()
        .scope()
        .with(SessionLock::lock)
        .expect("the lock is asked for");
    harness.app().scope().with(|| {
        lock.open_surface(MonitorId::new(1), zgui_elements::column);
        lock.open_surface(MonitorId::new(2), zgui_elements::column);
    });
    harness.settle(8);
    lock
}

#[test]
fn a_lock_opens_a_window_on_each_output_and_unlocks_after_the_desktop_confirms() {
    let mut harness = application(true);
    let lock = lock(&mut harness);
    assert_eq!(
        harness.platform().session_lock(),
        SessionLockState::Requested
    );
    let targets: Vec<_> = harness
        .platform()
        .offscreens()
        .iter()
        .map(|surface| surface.requested_attributes().role)
        .collect();
    assert_eq!(
        targets,
        [
            SurfaceRole::Lock(MonitorTarget::Id(MonitorId::new(1))),
            SurfaceRole::Lock(MonitorTarget::Id(MonitorId::new(2))),
        ]
    );
    assert!(lock.windows().iter().all(|window| window.is_open()));
    assert!(
        lock.windows()
            .iter()
            .all(|window| !window.focused().get_untracked()),
        "the desktop gives the keyboard"
    );

    assert!(!lock.unlock(), "an unconfirmed lock cannot be unlocked");
    assert!(!lock.locked().get_untracked());

    harness.platform().confirm_lock();
    harness.settle(4);
    assert!(lock.locked().get_untracked());

    assert!(lock.unlock());
    harness.settle(4);
    assert!(!lock.locked().get_untracked());
    assert!(!lock.finished().get_untracked());
    assert_eq!(
        harness.platform().session_lock(),
        SessionLockState::Unlocked
    );
    assert!(harness.platform().offscreens().is_empty());
    assert!(
        lock.windows().iter().all(|window| {
            window.close_reason().get_untracked() == Some(CloseReason::Application)
        })
    );
    assert!(
        !lock.unlock(),
        "a lock that is over cannot be unlocked again"
    );
}

#[test]
fn an_output_keeps_the_lock_window_it_has() {
    let mut harness = application(true);
    let lock = lock(&mut harness);
    // Asked for again, as an effect over a monitor list that restates an output does.
    let again = harness.app().scope().with(|| {
        lock.open_surface(MonitorId::new(1), zgui_elements::column);
        lock.open_surface(MonitorId::new(1), zgui_elements::column)
    });
    harness.settle(4);
    assert_eq!(again, lock.windows()[0], "the output's window answers");
    assert_eq!(lock.windows().len(), 2);
    assert_eq!(harness.platform().offscreens().len(), 2);
}

#[test]
fn one_lock_is_held_at_a_time() {
    let mut harness = application(true);
    let _lock = lock(&mut harness);
    let again = harness.app().scope().with(SessionLock::lock);
    assert_eq!(again.err(), Some(LockError::Held));
}

#[test]
fn a_lock_the_desktop_ends_closes_its_windows() {
    let mut harness = application(true);
    let lock = lock(&mut harness);
    harness.platform().confirm_lock();
    harness.settle(4);

    harness.platform().finish_lock();
    harness.settle(4);
    assert!(lock.finished().get_untracked());
    assert!(!lock.locked().get_untracked());
    assert!(harness.platform().offscreens().is_empty());
    assert!(
        lock.windows()
            .iter()
            .all(|window| { window.close_reason().get_untracked() == Some(CloseReason::Desktop) })
    );

    // A new lock can be asked for after the desktop ended the last one.
    let again = harness.app().scope().with(SessionLock::lock);
    assert!(again.is_ok());
}

#[test]
fn a_desktop_that_cannot_lock_ends_the_lock_at_once() {
    let mut harness = application(false);
    let lock = lock(&mut harness);
    assert!(lock.finished().get_untracked());
    assert!(lock.windows().iter().all(|window| !window.is_open()));
    assert!(harness.platform().offscreens().is_empty());
}

#[test]
fn a_lock_window_opened_after_the_lock_is_over_does_not_open() {
    let mut harness = application(true);
    let lock = lock(&mut harness);
    harness.platform().finish_lock();
    harness.settle(4);
    let late = harness
        .app()
        .scope()
        .with(|| lock.open_surface(MonitorId::new(1), zgui_elements::column));
    harness.settle(4);
    assert!(!late.is_open());
    assert_eq!(
        late.close_reason().get_untracked(),
        Some(CloseReason::Desktop),
        "the handle of a window that could not open hears that it closed"
    );
    assert!(harness.platform().offscreens().is_empty());
}

#[test]
fn a_refused_lock_with_no_window_leaves_the_application_running() {
    // The default policy ends the application when its last window closes. A refused lock that
    // had no window closes none, so the application hears of the refusal.
    let handler = App::new()
        .without_primary_window()
        .with_renderer(Box::new(support::capture))
        .into_handler(|_cx: &mut zgui_view::BuildCx<'_>| unreachable!("there is no primary window"))
        .expect("the reactive runtime installs");
    let mut harness = Harness::over(
        Headless::new().with_capabilities(PlatformCapabilities::none()),
        handler,
    );
    harness.settle(4);
    let lock = harness
        .app()
        .scope()
        .with(SessionLock::lock)
        .expect("the lock is asked for");
    harness.settle(4);
    assert!(lock.finished().get_untracked());
    assert!(!harness.platform().is_exiting());
}

#[test]
fn each_lock_window_reports_its_first_frame_on_the_screen() {
    let mut harness = application(true);
    let lock = harness
        .app()
        .scope()
        .with(SessionLock::lock)
        .expect("the lock is asked for");
    let window = harness
        .app()
        .scope()
        .with(|| lock.open_surface(MonitorId::new(1), zgui_elements::column));
    assert!(
        !window.presented().get_untracked(),
        "a window that drew nothing shows nothing"
    );
    harness.settle(8);
    assert!(
        window.presented().get_untracked(),
        "a platform without presentation reports counts the first frame as shown"
    );
}
