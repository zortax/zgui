//! TalkBack through `accesskit_android`, injected into the NativeActivity's content view.
//!
//! `accesskit_winit` expects a GameActivity on Android, so this takes its place with the same
//! constructor and methods. Requests from TalkBack arrive as the same events the desktop adapters
//! send, so the rest of the loop treats every platform alike.
//!
//! NativeActivity delivers input to native code first, and winit consumes hover events, so touch
//! exploration does not reach the adapter. Swipe navigation, reading and double-tap activation work
//! through the node tree alone.

use accesskit::{ActionHandler, ActionRequest, ActivationHandler, TreeUpdate};
use accesskit_android::InjectingAdapter;
use accesskit_android::jni::JavaVM;
use accesskit_android::jni::objects::JObject;
use accesskit_winit::{Event, WindowEvent as A11yEvent};
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::platform::android::ActiveEventLoopExtAndroid;
use winit::window::{Window, WindowId};

/// `android.R.id.content`, the view NativeActivity draws into.
const CONTENT_VIEW: i32 = 0x0102_0002;

/// Sends TalkBack's requests to the event loop as AccessKit events.
struct Forward<T: From<Event> + Send + 'static> {
    window_id: WindowId,
    proxy: EventLoopProxy<T>,
}

impl<T: From<Event> + Send + 'static> Forward<T> {
    fn send(&self, window_event: A11yEvent) {
        let event = Event {
            window_id: self.window_id,
            window_event,
        };
        let _ = self.proxy.send_event(event.into());
    }
}

impl<T: From<Event> + Send + 'static> ActivationHandler for Forward<T> {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        self.send(A11yEvent::InitialTreeRequested);
        None
    }
}

impl<T: From<Event> + Send + 'static> ActionHandler for Forward<T> {
    fn do_action(&mut self, request: ActionRequest) {
        self.send(A11yEvent::ActionRequested(request));
    }
}

/// One window's TalkBack adapter; empty when the content view could not be reached.
pub(crate) struct Adapter(Option<InjectingAdapter>);

impl Adapter {
    /// Injects the adapter into the activity's content view.
    pub(crate) fn with_event_loop_proxy<T: From<Event> + Send + 'static>(
        event_loop: &ActiveEventLoop,
        window: &Window,
        proxy: EventLoopProxy<T>,
    ) -> Self {
        let window_id = window.id();
        let app = event_loop.android_app();
        // SAFETY: AndroidApp keeps the VM alive for the life of the process.
        let Ok(vm) = (unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) }) else {
            return Self(None);
        };
        let Ok(mut env) = vm.attach_current_thread_permanently() else {
            return Self(None);
        };
        // SAFETY: the activity reference belongs to AndroidApp and outlives this call; JObject
        // does not delete a reference when dropped.
        let activity = unsafe { JObject::from_raw(app.activity_as_ptr().cast()) };
        let view = env
            .call_method(
                &activity,
                "findViewById",
                "(I)Landroid/view/View;",
                &[CONTENT_VIEW.into()],
            )
            .and_then(|value| value.l());
        let view = match view {
            Ok(view) if !view.is_null() => view,
            _ => {
                let _ = env.exception_clear();
                tracing::warn!("Android content view not found; TalkBack unavailable");
                return Self(None);
            }
        };
        let forward = || Forward {
            window_id,
            proxy: proxy.clone(),
        };
        Self(Some(InjectingAdapter::new(
            &mut env,
            &view,
            forward(),
            forward(),
        )))
    }

    /// Builds and sends a tree update while TalkBack is listening.
    pub(crate) fn update_if_active(&mut self, build: impl FnOnce() -> TreeUpdate) {
        if let Some(adapter) = &mut self.0 {
            adapter.update_if_active(build);
        }
    }

    /// Window events carry nothing the Android adapter needs.
    pub(crate) fn process_event(&mut self, _window: &Window, _event: &WindowEvent) {}
}
