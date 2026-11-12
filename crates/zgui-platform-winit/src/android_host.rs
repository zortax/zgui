//! Android host bridge. Rust loads the library's `ZguiHost` Java class from embedded dex, so
//! applications use stock `NativeActivity` and need no Java code.

use crate::waker::UserEvent;
use jni::objects::{Global, JClass, JObject, JString, JValue};
use jni::{Env, EnvUnowned, JavaVM, jni_sig, jni_str};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use winit::event_loop::EventLoopProxy;
use winit::platform::android::activity::AndroidApp;
use zgui_geom::{DevicePx, Edges};
use zgui_platform::{NativeTextInput, SurfaceId};

/// `ZguiHost` compiled by `android/build-dex.sh`.
static DEX: &[u8] = include_bytes!("../android/zgui-host.dex");

/// First font file the application registered; the native editor measures text with it.
static FONT: OnceLock<Arc<dyn AsRef<[u8]> + Send + Sync>> = OnceLock::new();

/// Sets the font the native editor measures with. Only the first call has an effect.
pub(crate) fn editor_font(data: Arc<dyn AsRef<[u8]> + Send + Sync>) {
    let _ = FONT.set(data);
}

struct Bridge {
    app: AndroidApp,
    host: Global<JObject<'static>>,
    proxy: EventLoopProxy<UserEvent>,
    acknowledged: u64,
    last: Option<(SurfaceId, NativeTextInput)>,
    touching: bool,
}
static BRIDGE: OnceLock<Mutex<Bridge>> = OnceLock::new();

fn call<T>(
    app: &AndroidApp,
    f: impl FnOnce(&mut Env<'_>, &JObject<'_>) -> jni::errors::Result<T>,
) -> jni::errors::Result<T> {
    // SAFETY: AndroidApp keeps the VM and activity's global reference alive throughout the call.
    let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) };
    vm.attach_current_thread(|env| {
        let raw = app.activity_as_ptr().cast();
        // SAFETY: reference belongs to AndroidApp. Cast borrows it and does not delete it.
        let activity = unsafe { env.as_cast_raw::<Global<JObject>>(&raw)? };
        f(env, activity.as_ref())
    })
}

/// Calls `f` with the `ZguiHost` instance.
fn host<T>(
    bridge: &Bridge,
    f: impl FnOnce(&mut Env<'_>, &JObject<'_>) -> jni::errors::Result<T>,
) -> jni::errors::Result<T> {
    call(&bridge.app, |env, _| f(env, bridge.host.as_ref()))
}

/// Loads `ZguiHost` from [`DEX`] and creates it for the activity.
fn load(
    env: &mut Env<'_>,
    activity: &JObject<'_>,
    font: Option<&str>,
) -> jni::errors::Result<Global<JObject<'static>>> {
    let parent = env
        .call_method(
            activity,
            jni_str!("getClassLoader"),
            jni_sig!("()Ljava/lang/ClassLoader;"),
            &[],
        )?
        .l()?;
    // SAFETY: DEX is static and Java only reads it.
    let buffer = unsafe { env.new_direct_byte_buffer(DEX.as_ptr().cast_mut(), DEX.len())? };
    let loader = env.new_object(
        jni_str!("dalvik/system/InMemoryDexClassLoader"),
        jni_sig!("(Ljava/nio/ByteBuffer;Ljava/lang/ClassLoader;)V"),
        &[JValue::Object(buffer.as_ref()), JValue::Object(&parent)],
    )?;
    let name = JString::from_str(env, "dev.zgui.android.ZguiHost")?;
    let class = env
        .call_method(
            &loader,
            jni_str!("loadClass"),
            jni_sig!("(Ljava/lang/String;)Ljava/lang/Class;"),
            &[JValue::Object(name.as_ref())],
        )?
        .l()?;
    let class = env.cast_local::<JClass>(class)?;
    // SAFETY: Java static native declaration and Rust callback have identical JNI signature.
    unsafe {
        let method = jni::NativeMethod::from_raw_parts(
            jni_str!("changed"),
            jni_str!("(JJJLjava/lang/String;II)V"),
            changed as *mut core::ffi::c_void,
        );
        let destroyed = jni::NativeMethod::from_raw_parts(
            jni_str!("destroyed"),
            jni_str!("()V"),
            destroyed as *mut core::ffi::c_void,
        );
        let insets = jni::NativeMethod::from_raw_parts(
            jni_str!("insetsChanged"),
            jni_str!("(IIIII)V"),
            insets_changed as *mut core::ffi::c_void,
        );
        let rate = jni::NativeMethod::from_raw_parts(
            jni_str!("refreshRate"),
            jni_str!("(F)V"),
            refresh_rate_changed as *mut core::ffi::c_void,
        );
        let configuration = jni::NativeMethod::from_raw_parts(
            jni_str!("configurationChanged"),
            jni_str!("(ZFZ)V"),
            configuration_changed as *mut core::ffi::c_void,
        );
        let tilted = jni::NativeMethod::from_raw_parts(
            jni_str!("tilted"),
            jni_str!("(FF)V"),
            tilted as *mut core::ffi::c_void,
        );
        let picked = jni::NativeMethod::from_raw_parts(
            jni_str!("picked"),
            jni_str!("(Ljava/lang/String;)V"),
            picked as *mut core::ffi::c_void,
        );
        env.register_native_methods(
            &class,
            &[
                method,
                destroyed,
                insets,
                rate,
                configuration,
                tilted,
                picked,
            ],
        )?;
    }
    let font = match font {
        Some(path) => JString::from_str(env, path)?.into(),
        None => JObject::null(),
    };
    let host = env.new_object(
        &class,
        jni_sig!("(Landroid/app/Activity;Ljava/lang/String;)V"),
        &[JValue::Object(activity), JValue::Object(&font)],
    )?;
    env.new_global_ref(host)
}

/// Writes the editor font where Android's `Typeface` can read it.
fn font_file(app: &AndroidApp) -> Option<String> {
    let data = FONT.get()?;
    let path = app.internal_data_path()?.join("zgui-editor-font");
    match std::fs::write(&path, data.as_ref().as_ref()) {
        Ok(()) => path.into_os_string().into_string().ok(),
        Err(error) => {
            tracing::warn!(%error, "native editor font could not be written");
            None
        }
    }
}

pub(crate) fn attach(app: AndroidApp, proxy: EventLoopProxy<UserEvent>) {
    let font = font_file(&app);
    match call(&app, |env, activity| {
        let result = load(env, activity, font.as_deref());
        if result.is_err() {
            env.exception_clear();
        }
        result
    }) {
        Ok(host) => {
            let _ = BRIDGE.set(Mutex::new(Bridge {
                app,
                host,
                proxy,
                acknowledged: 0,
                last: None,
                touching: false,
            }));
        }
        // Without the host, NativeActivity keeps basic key input.
        Err(error) => tracing::warn!(%error, "Android host could not load"),
    }
}

pub(crate) fn available() -> bool {
    BRIDGE.get().is_some()
}

pub(crate) fn acknowledge(surface: SurfaceId, target: u64, revision: u64) {
    if let Some(bridge) = BRIDGE.get() {
        let mut bridge = bridge.lock().expect("Android editor lock");
        if bridge
            .last
            .as_ref()
            .is_some_and(|(owner, state)| *owner == surface && state.target == target)
        {
            bridge.acknowledged = bridge.acknowledged.max(revision);
        }
    }
}

pub(crate) fn update(surface: SurfaceId, state: Option<NativeTextInput>, scale: f64) {
    let Some(bridge) = BRIDGE.get() else {
        return;
    };
    let mut bridge = bridge.lock().expect("Android editor lock");
    let next = state.map(|state| (surface, state));
    if bridge.last == next {
        return;
    }
    if bridge.last.as_ref().map(|(_, s)| s.target) != next.as_ref().map(|(_, s)| s.target) {
        bridge.acknowledged = 0;
        bridge.touching = false;
    }
    let ack = bridge.acknowledged;
    let result = host(&bridge, |env, host| {
        let state = next.as_ref().map(|(_, s)| s);
        let value = state.map_or("", |s| s.text.as_str());
        let text = JString::from_str(env, value)?;
        let (anchor, focus) = state.map_or((0, 0), |s| s.selection);
        let utf16 =
            |offset: usize| value.get(..offset).unwrap_or(value).encode_utf16().count() as i32;
        let rect = state.map(|s| s.bounds).unwrap_or_default();
        let scale = scale as f32;
        env.call_method(
            host,
            jni_str!("update"),
            jni_sig!("(JJJLjava/lang/String;IIFFFFFZZ)V"),
            &[
                JValue::Long(surface.raw() as i64),
                JValue::Long(state.map_or(0, |s| s.target) as i64),
                JValue::Long(ack as i64),
                JValue::Object(text.as_ref()),
                JValue::Int(utf16(anchor)),
                JValue::Int(utf16(focus)),
                JValue::Float(rect.origin.x.0 * scale),
                JValue::Float(rect.origin.y.0 * scale),
                JValue::Float(rect.size.width.0 * scale),
                JValue::Float(rect.size.height.0 * scale),
                JValue::Float(state.map_or(16.0, |s| s.font_size.0) * scale),
                JValue::Bool(state.is_some_and(|s| s.secret)),
                JValue::Bool(state.is_some_and(|s| s.multiline)),
            ],
        )?;
        Ok(())
    });
    match result {
        Ok(()) => bridge.last = next,
        Err(error) => tracing::error!(%error, "Android editor update failed"),
    }
}

pub(crate) fn show() {
    let Some(bridge) = BRIDGE.get() else {
        return;
    };
    let bridge = bridge.lock().expect("Android editor lock");
    if let Err(error) = host(&bridge, |env, host| {
        env.call_method(host, jni_str!("showKeyboard"), jni_sig!("()V"), &[])?;
        Ok(())
    }) {
        tracing::error!(%error, "Android keyboard request failed");
    }
}

pub(crate) fn clipboard(text: Option<(&str, bool)>) -> Result<String, String> {
    let bridge = BRIDGE
        .get()
        .ok_or("Android host unavailable")?
        .lock()
        .map_err(|e| e.to_string())?;
    host(&bridge, |env, host| {
        if let Some((text, secret)) = text {
            let text = JString::from_str(env, text)?;
            env.call_method(
                host,
                jni_str!("setClipboard"),
                jni_sig!("(Ljava/lang/String;Z)V"),
                &[JValue::Object(text.as_ref()), JValue::Bool(secret)],
            )?;
            Ok(String::new())
        } else {
            let text = env
                .call_method(
                    host,
                    jni_str!("clipboard"),
                    jni_sig!("()Ljava/lang/String;"),
                    &[],
                )?
                .l()?;
            let text = env.cast_local::<JString>(text)?;
            Ok(text.to_string())
        }
    })
    .map_err(|e| e.to_string())
}

// SAFETY: signature matches ZguiHost.changed. EnvUnowned bounds all JNI references to caller.
extern "system" fn changed<'caller>(
    mut env: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    surface: i64,
    target: i64,
    revision: i64,
    text: JString<'caller>,
    anchor: i32,
    focus: i32,
) {
    env.with_env(|_env| -> jni::errors::Result<()> {
        let text = text.to_string();
        let to_byte = |offset: i32| {
            let mut units = 0;
            for (byte, ch) in text.char_indices() {
                if units >= offset.max(0) as usize {
                    return byte;
                }
                units += ch.len_utf16();
            }
            text.len()
        };
        let selection = (to_byte(anchor), to_byte(focus));
        if let Some(bridge) = BRIDGE.get() {
            let bridge = bridge.lock().expect("Android editor lock");
            let _ = bridge.proxy.send_event(UserEvent::NativeText {
                surface: SurfaceId::new(surface as u64),
                target: target as u64,
                revision: revision as u64,
                text,
                selection,
            });
        }
        Ok(())
    })
    .resolve::<jni::errors::LogErrorAndDefault>();
}

/// Latest system insets in physical pixels: safe area, then keyboard height.
static INSETS: Mutex<Option<(Edges<DevicePx>, DevicePx)>> = Mutex::new(None);

/// Display refresh rate the host selected, in millihertz; zero until reported.
static REFRESH: AtomicU32 = AtomicU32::new(0);

/// Latest system insets, when the host has reported any.
pub(crate) fn insets() -> Option<(Edges<DevicePx>, DevicePx)> {
    *INSETS.lock().expect("Android insets lock")
}

/// Display refresh rate the host selected.
pub(crate) fn refresh_rate() -> Option<u32> {
    Some(REFRESH.load(Ordering::Relaxed)).filter(|rate| *rate > 0)
}

/// Calls `f` with the `ZguiHost` instance and logs a failure as `what`.
fn with_host(what: &str, f: impl FnOnce(&mut Env<'_>, &JObject<'_>) -> jni::errors::Result<()>) {
    let Some(bridge) = BRIDGE.get() else {
        return;
    };
    let bridge = bridge.lock().expect("Android host lock");
    if let Err(error) = host(&bridge, f) {
        tracing::error!(%error, what, "Android host call failed");
    }
}

/// Moves the application to the background, as Android's Back does for a root activity.
pub(crate) fn go_back() {
    with_host("back", |env, host| {
        env.call_method(host, jni_str!("goBack"), jni_sig!("()V"), &[])?;
        Ok(())
    });
}

/// Sets the theme the application shows, for the system bar icons: `None` follows the system.
pub(crate) fn bar_theme(dark: Option<bool>) {
    let theme = match dark {
        None => 0,
        Some(false) => 1,
        Some(true) => 2,
    };
    with_host("bar theme", |env, host| {
        env.call_method(
            host,
            jni_str!("barTheme"),
            jni_sig!("(I)V"),
            &[JValue::Int(theme)],
        )?;
        Ok(())
    });
}

/// Plays haptic feedback, if the system haptics setting allows it.
pub(crate) fn haptic(kind: i32) {
    with_host("haptic", |env, host| {
        env.call_method(
            host,
            jni_str!("haptic"),
            jni_sig!("(I)V"),
            &[JValue::Int(kind)],
        )?;
        Ok(())
    });
}

/// Opens Android's share sheet with `text`.
pub(crate) fn share(text: &str) {
    with_host("share", |env, host| {
        let text = JString::from_str(env, text)?;
        env.call_method(
            host,
            jni_str!("share"),
            jni_sig!("(Ljava/lang/String;)V"),
            &[JValue::Object(text.as_ref())],
        )?;
        Ok(())
    });
}

/// Opens `url` in the application that handles it.
pub(crate) fn open_url(url: &str) {
    with_host("open URL", |env, host| {
        let url = JString::from_str(env, url)?;
        env.call_method(
            host,
            jni_str!("openUrl"),
            jni_sig!("(Ljava/lang/String;)V"),
            &[JValue::Object(url.as_ref())],
        )?;
        Ok(())
    });
}

/// Receives gravity tilt in units of g: x to the right, y up the screen.
type TiltCallback = Box<dyn Fn(f32, f32) + Send>;
static TILT: Mutex<Option<TiltCallback>> = Mutex::new(None);

/// Starts tilt reports to `callback` on a sensor thread, or stops them with `None`.
pub(crate) fn on_tilt(callback: Option<TiltCallback>) {
    let on = callback.is_some();
    *TILT.lock().expect("Android tilt lock") = callback;
    with_host("tilt", |env, host| {
        env.call_method(
            host,
            jni_str!("tilt"),
            jni_sig!("(Z)V"),
            &[JValue::Bool(on)],
        )?;
        Ok(())
    });
}

/// Receives the picked image's PNG path, or `None` when cancelled.
type PickCallback = Box<dyn FnOnce(Option<std::path::PathBuf>) + Send>;
static PICK: Mutex<Option<PickCallback>> = Mutex::new(None);

/// Opens the system photo picker; `callback` runs on another thread with the result.
pub(crate) fn pick_image(callback: PickCallback) {
    *PICK.lock().expect("Android pick lock") = Some(callback);
    with_host("pick image", |env, host| {
        env.call_method(host, jni_str!("pickImage"), jni_sig!("()V"), &[])?;
        Ok(())
    });
}

/// Latest system dark mode, font scale and right-to-left layout.
static CONFIGURATION: Mutex<Option<(bool, f32, bool)>> = Mutex::new(None);

/// Latest system dark mode, font scale and right-to-left layout, when the host has reported them.
pub(crate) fn configuration() -> Option<(bool, f32, bool)> {
    *CONFIGURATION.lock().expect("Android configuration lock")
}

// SAFETY: signature matches ZguiHost.configurationChanged.
extern "system" fn configuration_changed<'caller>(
    _env: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    dark: bool,
    font_scale: f32,
    rtl: bool,
) {
    *CONFIGURATION.lock().expect("Android configuration lock") = Some((dark, font_scale, rtl));
    if let Some(bridge) = BRIDGE.get() {
        let bridge = bridge.lock().expect("Android host lock");
        let _ = bridge.proxy.send_event(UserEvent::Configuration);
    }
}

// SAFETY: signature matches ZguiHost.tilted.
extern "system" fn tilted<'caller>(
    _env: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    x: f32,
    y: f32,
) {
    if let Some(callback) = TILT.lock().expect("Android tilt lock").as_ref() {
        callback(x, y);
    }
}

// SAFETY: signature matches ZguiHost.picked. EnvUnowned bounds all JNI references to caller.
extern "system" fn picked<'caller>(
    mut env: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    path: JString<'caller>,
) {
    env.with_env(|_env| -> jni::errors::Result<()> {
        let path = (!path.is_null()).then(|| std::path::PathBuf::from(path.to_string()));
        if let Some(callback) = PICK.lock().expect("Android pick lock").take() {
            callback(path);
        }
        Ok(())
    })
    .resolve::<jni::errors::LogErrorAndDefault>();
}

// SAFETY: signature matches ZguiHost.insetsChanged.
extern "system" fn insets_changed<'caller>(
    _env: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    top: i32,
    right: i32,
    bottom: i32,
    left: i32,
    keyboard: i32,
) {
    let px = |value: i32| DevicePx(value.max(0) as f32);
    let insets = (
        Edges::new(px(top), px(right), px(bottom), px(left)),
        px(keyboard),
    );
    *INSETS.lock().expect("Android insets lock") = Some(insets);
    if let Some(bridge) = BRIDGE.get() {
        let bridge = bridge.lock().expect("Android host lock");
        let _ = bridge.proxy.send_event(UserEvent::Insets);
    }
}

// SAFETY: signature matches ZguiHost.refreshRate.
extern "system" fn refresh_rate_changed<'caller>(
    _env: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    hertz: f32,
) {
    let rate = (hertz * 1000.0).round() as u32;
    if REFRESH.swap(rate, Ordering::Relaxed) != rate {
        tracing::info!(hertz, "display refresh rate");
    }
}

// SAFETY: signature matches ZguiHost.destroyed.
extern "system" fn destroyed<'caller>(_env: EnvUnowned<'caller>, _class: JClass<'caller>) {
    if let Some(bridge) = BRIDGE.get() {
        let bridge = bridge.lock().expect("Android host lock");
        let _ = bridge.proxy.send_event(UserEvent::Destroyed);
    }
}

/// NativeActivity bypasses Java View touch dispatch. Forward focused-field gestures explicitly.
pub(crate) fn pointer(surface: SurfaceId, event: &zgui_platform::SurfaceEvent, scale: f64) -> bool {
    use zgui_vocab::PointerAction;
    let zgui_platform::SurfaceEvent::Pointer { action, event, .. } = event else {
        return false;
    };
    let Some(bridge) = BRIDGE.get() else {
        return false;
    };
    let mut bridge = bridge.lock().expect("Android editor lock");
    let Some((owner, field)) = bridge.last.as_ref() else {
        return false;
    };
    if *owner != surface {
        return false;
    }
    let bounds = field.bounds;
    let x = event.position.x.0;
    let y = event.position.y.0;
    let inside = x >= bounds.origin.x.0
        && x < bounds.origin.x.0 + bounds.size.width.0
        && y >= bounds.origin.y.0
        && y < bounds.origin.y.0 + bounds.size.height.0;
    let code = match action {
        PointerAction::Pressed if inside => {
            bridge.touching = true;
            0
        }
        PointerAction::Moved if bridge.touching => 2,
        PointerAction::Released if bridge.touching => {
            bridge.touching = false;
            1
        }
        PointerAction::Cancelled if bridge.touching => {
            bridge.touching = false;
            3
        }
        _ => return false,
    };
    let scale = scale as f32;
    if let Err(error) = host(&bridge, |env, host| {
        env.call_method(
            host,
            jni_str!("touch"),
            jni_sig!("(IFF)V"),
            &[
                JValue::Int(code),
                JValue::Float((x - bounds.origin.x.0) * scale),
                JValue::Float((y - bounds.origin.y.0) * scale),
            ],
        )?;
        Ok(())
    }) {
        tracing::error!(%error, "Android editor touch failed");
    }
    true
}

/// Asks `ZguiHost` to post a system notification after `delay`.
pub(crate) fn notify(title: &str, text: &str, delay: std::time::Duration) -> Result<(), String> {
    let bridge = BRIDGE
        .get()
        .ok_or("Android host unavailable")?
        .lock()
        .map_err(|e| e.to_string())?;
    host(&bridge, |env, host| {
        let title = JString::from_str(env, title)?;
        let text = JString::from_str(env, text)?;
        env.call_method(
            host,
            jni_str!("notify"),
            jni_sig!("(Ljava/lang/String;Ljava/lang/String;J)V"),
            &[
                JValue::Object(title.as_ref()),
                JValue::Object(text.as_ref()),
                JValue::Long(delay.as_millis().try_into().unwrap_or(i64::MAX)),
            ],
        )?;
        Ok(())
    })
    .map_err(|e| e.to_string())
}
