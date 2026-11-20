//! What an application releases by the time its run returns.
//!
//! Every scope a view created is disposed of while the application's own scope is still alive, so
//! a cleanup can still read what the application provided.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use zgui::platform::{AppHandler, PlatformError, Surface};
use zgui::prelude::*;
use zgui::runtime::AppError;

/// How many times the innermost component was built.
static BUILT: AtomicUsize = AtomicUsize::new(0);
/// How many times its cleanup ran.
static CLEANED: AtomicUsize = AtomicUsize::new(0);
/// How many of those cleanups still found the application's signal alive.
static SAW_SHARED: AtomicUsize = AtomicUsize::new(0);

/// A signal the application provides above every window.
#[derive(Clone, Copy)]
struct Shared(RwSignal<u32, LocalStorage>);

/// A renderer that draws nowhere.
type Nowhere = zgui_testkit_compositor::NullRenderer;

/// The innermost component. Its cleanup reads the application's signal.
#[component]
fn Deep() -> impl IntoView {
    BUILT.fetch_add(1, Ordering::SeqCst);
    let shared = expect_context::<Shared>();
    on_cleanup_local(move || {
        if shared.0.try_get_untracked().is_some() {
            SAW_SHARED.fetch_add(1, Ordering::SeqCst);
        }
        CLEANED.fetch_add(1, Ordering::SeqCst);
    });
    view! { text {"deep"} }
}

/// A component that holds `Deep` in a reactive hole of its own.
#[component]
fn Leaf(n: u32) -> impl IntoView {
    view! { column(attr:data-n = Some(n.to_string())) { {move || view! { Deep() }} } }
}

/// Rebuilds `Leaf` once, on the first flush.
#[component]
fn Root() -> impl IntoView {
    let tick = RwSignal::new_local(0_u32);
    spawn_local(async move { tick.set(1) });
    view! { column { {move || view! { Leaf(n = tick.get()) }} } }
}

/// Drives the application over buffers for a few turns and stops.
fn buffers(handler: Box<dyn AppHandler>) -> Result<(), PlatformError> {
    let mut harness = zgui_platform_headless::Harness::new(handler);
    harness.settle(8);
    harness.shut_down();
    Ok(())
}

#[test]
fn every_scope_is_disposed_of_before_the_run_returns() {
    app()
        .with_title("shutdown")
        .with_size(320.0, 200.0)
        .with_context(|| provide_context(Shared(RwSignal::new_local(7))))
        .with_renderer(Box::new(|_surface: &Arc<dyn Surface>, target| {
            let mut renderer = Nowhere::default();
            compositor_api::Renderer::configure(&mut renderer, target);
            Ok::<_, AppError>(Box::new(renderer) as Box<dyn compositor_api::Renderer>)
        }))
        .run_on(buffers, || view! { Root() })
        .expect("the application ran");

    let built = BUILT.load(Ordering::SeqCst);
    assert!(
        built >= 2,
        "the hole was rebuilt, so `Deep` was built twice"
    );
    assert_eq!(
        CLEANED.load(Ordering::SeqCst),
        built,
        "every build of `Deep` was cleaned up before the run returned"
    );
    assert_eq!(
        SAW_SHARED.load(Ordering::SeqCst),
        built,
        "and every cleanup ran while the application's scope was alive"
    );
}
