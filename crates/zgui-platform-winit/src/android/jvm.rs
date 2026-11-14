//! Calls into the `Zgui` Java class and replies from it.
//!
//! Rust calls `Zgui.call(op, token, args)`. An operation that finishes later replies through
//! `Zgui.reply(token, …)` to the callback registered under `token`. Strings carry all values.

use jni::objects::{Global, JByteArray, JClass, JObject, JObjectArray, JString, JValue};
use jni::{Env, EnvUnowned, JavaVM, jni_sig, jni_str};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

static VM: OnceLock<JavaVM> = OnceLock::new();
static CLASS: OnceLock<Global<JClass<'static>>> = OnceLock::new();

/// Result codes of `Zgui.reply`.
pub(crate) const OK: i32 = 0;
pub(crate) const CANCELLED: i32 = 1;

/// One reply from Java.
pub(crate) struct Reply {
    pub(crate) code: i32,
    pub(crate) text: Vec<String>,
    pub(crate) bytes: Option<Vec<u8>>,
}

impl Reply {
    /// `Ok` with the text for code `OK`, `Err` with the message for an error.
    /// A cancelled operation gives `Ok(None)`.
    pub(crate) fn result(self) -> Result<Option<Vec<String>>, String> {
        match self.code {
            OK => Ok(Some(self.text)),
            CANCELLED => Ok(None),
            _ => Err(self.text.into_iter().next().unwrap_or_default()),
        }
    }

    /// Text field `index`, empty when absent.
    pub(crate) fn get(&self, index: usize) -> &str {
        self.text.get(index).map_or("", String::as_str)
    }
}

type Once = Box<dyn FnOnce(Reply) + Send>;
type Many = Arc<Mutex<Box<dyn FnMut(Reply) + Send>>>;
enum Slot {
    Once(Once),
    Many(Many),
}
static SLOTS: Mutex<Option<HashMap<u64, Slot>>> = Mutex::new(None);
static NEXT: AtomicU64 = AtomicU64::new(1);

fn insert(slot: Slot) -> u64 {
    let token = NEXT.fetch_add(1, Ordering::Relaxed);
    SLOTS
        .lock()
        .expect("Android reply lock")
        .get_or_insert_with(HashMap::new)
        .insert(token, slot);
    token
}

/// Removes the callback of `token`; later replies to it are dropped.
pub(crate) fn forget(token: u64) {
    if let Some(slots) = SLOTS.lock().expect("Android reply lock").as_mut() {
        slots.remove(&token);
    }
}

/// Starts `op`, which replies once to `callback`.
pub(crate) fn once(
    op: &str,
    args: &[&str],
    callback: impl FnOnce(Reply) + Send + 'static,
) -> Result<(), String> {
    let token = insert(Slot::Once(Box::new(callback)));
    call(op, token, args)
        .map(drop)
        .inspect_err(|_| forget(token))
}

/// Starts `op`, which replies to `callback` until the guard drops.
pub(crate) fn many(
    op: &str,
    args: &[&str],
    callback: impl FnMut(Reply) + Send + 'static,
) -> Result<super::Guard, String> {
    let token = insert(Slot::Many(Arc::new(Mutex::new(Box::new(callback)))));
    match call(op, token, args) {
        Ok(_) => Ok(super::Guard(token)),
        Err(error) => {
            forget(token);
            Err(error)
        }
    }
}

/// Runs `f` with the `Zgui` class on an attached thread. A Java exception becomes its message.
fn with_class<T>(
    f: impl FnOnce(&mut Env<'_>, &JClass<'_>) -> jni::errors::Result<T>,
) -> Result<T, String> {
    let (Some(vm), Some(class)) = (VM.get(), CLASS.get()) else {
        return Err("Android services unavailable".into());
    };
    vm.attach_current_thread(|env| -> jni::errors::Result<T> {
        f(env, class.as_ref()).or_else(|error| match error {
            jni::errors::Error::JavaException => env.exception_catch().and(Err(error)),
            error => Err(error),
        })
    })
    .map_err(|error| match error {
        jni::errors::Error::CaughtJavaException { name, msg, .. } if msg.is_empty() => name,
        jni::errors::Error::CaughtJavaException { msg, .. } => msg,
        error => error.to_string(),
    })
}

fn strings<'local>(
    env: &mut Env<'local>,
    items: &[&str],
) -> jni::errors::Result<JObjectArray<'local>> {
    let array = env.new_object_array(
        items.len() as i32,
        jni_str!("java/lang/String"),
        JObject::null(),
    )?;
    for (index, item) in items.iter().enumerate() {
        let item = JString::from_str(env, item)?;
        array.set_element(env, index, &item)?;
    }
    Ok(array)
}

fn read_strings(env: &mut Env<'_>, array: &JObjectArray<'_>) -> jni::errors::Result<Vec<String>> {
    if array.is_null() {
        return Ok(Vec::new());
    }
    (0..array.len(env)?)
        .map(|index| {
            let item = array.get_element(env, index)?;
            if item.is_null() {
                return Ok(String::new());
            }
            Ok(env.cast_local::<JString>(item)?.to_string())
        })
        .collect()
}

/// Calls `Zgui.call(op, token, args)` and returns its strings; `null` gives none.
pub(crate) fn call(op: &str, token: u64, args: &[&str]) -> Result<Vec<String>, String> {
    with_class(|env, class| {
        let op = JString::from_str(env, op)?;
        let args = strings(env, args)?;
        let result = env
            .call_static_method(
                class,
                jni_str!("call"),
                jni_sig!("(Ljava/lang/String;J[Ljava/lang/String;)[Ljava/lang/String;"),
                &[
                    JValue::Object(op.as_ref()),
                    JValue::Long(token as i64),
                    JValue::Object(args.as_ref()),
                ],
            )?
            .l()?;
        if result.is_null() {
            return Ok(Vec::new());
        }
        let result = env.cast_local::<JObjectArray>(result)?;
        read_strings(env, &result)
    })
}

/// Calls `Zgui.call` for an operation that returns at most one string.
pub(crate) fn call_one(op: &str, args: &[&str]) -> Result<String, String> {
    Ok(call(op, 0, args)?.into_iter().next().unwrap_or_default())
}

/// Calls `Zgui.bytes(op, args, data)`.
pub(crate) fn bytes(op: &str, args: &[&str], data: &[u8]) -> Result<Vec<u8>, String> {
    with_class(|env, class| {
        let op = JString::from_str(env, op)?;
        let args = strings(env, args)?;
        let data = env.byte_array_from_slice(data)?;
        let result = env
            .call_static_method(
                class,
                jni_str!("bytes"),
                jni_sig!("(Ljava/lang/String;[Ljava/lang/String;[B)[B"),
                &[
                    JValue::Object(op.as_ref()),
                    JValue::Object(args.as_ref()),
                    JValue::Object(data.as_ref()),
                ],
            )?
            .l()?;
        let result = env.cast_local::<JByteArray>(result)?;
        env.convert_byte_array(&result)
    })
}

/// Takes ownership of a file descriptor that Java detached for Rust.
pub(crate) fn file(fd: &str) -> Result<std::fs::File, String> {
    use std::os::fd::FromRawFd;
    let fd: i32 = fd.parse().map_err(|_| "Invalid file descriptor")?;
    if fd < 0 {
        return Err("Invalid file descriptor".into());
    }
    // SAFETY: Java detached this descriptor from its ParcelFileDescriptor, so no other owner
    // closes it.
    #[allow(unsafe_code)]
    Ok(unsafe { std::fs::File::from_raw_fd(fd) })
}

/// Registers the native methods of `Zgui` and stores the VM, once per process.
pub(crate) fn install(
    env: &mut Env<'_>,
    class: &JClass<'_>,
    context: &JObject<'_>,
) -> jni::errors::Result<()> {
    static INSTALLED: Mutex<bool> = Mutex::new(false);
    let mut installed = INSTALLED.lock().expect("Android install lock");
    if !*installed {
        let mut methods = super::input::methods();
        // SAFETY: each declaration in Zgui.java has this exact JNI signature.
        #[allow(unsafe_code)]
        unsafe {
            methods.push(jni::NativeMethod::from_raw_parts(
                jni_str!("reply"),
                jni_str!("(JZI[Ljava/lang/String;[B)V"),
                reply as *mut core::ffi::c_void,
            ));
            methods.push(jni::NativeMethod::from_raw_parts(
                jni_str!("event"),
                jni_str!("(Ljava/lang/String;[Ljava/lang/String;)V"),
                event as *mut core::ffi::c_void,
            ));
            env.register_native_methods(class, &methods)?;
        }
        let _ = VM.set(env.get_java_vm()?);
        let _ = CLASS.set(env.new_global_ref(class)?);
        *installed = true;
    }
    drop(installed);
    env.call_static_method(
        class,
        jni_str!("init"),
        jni_sig!("(Landroid/content/Context;)V"),
        &[JValue::Object(context)],
    )?;
    Ok(())
}

pub(crate) fn read_args(
    env: &mut Env<'_>,
    args: &JObjectArray<'_>,
) -> jni::errors::Result<Vec<String>> {
    read_strings(env, args)
}

// SAFETY: signature matches Zgui.reply.
extern "system" fn reply<'a>(
    mut env: EnvUnowned<'a>,
    _class: JClass<'a>,
    token: i64,
    last: bool,
    code: i32,
    text: JObjectArray<'a>,
    bytes: JByteArray<'a>,
) {
    env.with_env(|env| -> jni::errors::Result<()> {
        let reply = Reply {
            code,
            text: read_strings(env, &text)?,
            bytes: if bytes.is_null() {
                None
            } else {
                Some(env.convert_byte_array(&bytes)?)
            },
        };
        let token = token as u64;
        let slot = {
            let mut slots = SLOTS.lock().expect("Android reply lock");
            let slots = slots.get_or_insert_with(HashMap::new);
            match slots.get(&token) {
                Some(Slot::Many(callback)) if !last => Some(Slot::Many(callback.clone())),
                Some(_) => slots.remove(&token),
                None => None,
            }
        };
        match slot {
            Some(Slot::Once(callback)) => callback(reply),
            Some(Slot::Many(callback)) => (callback.lock().expect("Android callback lock"))(reply),
            None => {}
        }
        Ok(())
    })
    .resolve::<jni::errors::LogErrorAndDefault>();
}

// SAFETY: signature matches Zgui.event.
extern "system" fn event<'a>(
    mut env: EnvUnowned<'a>,
    _class: JClass<'a>,
    kind: JString<'a>,
    text: JObjectArray<'a>,
) {
    env.with_env(|env| -> jni::errors::Result<()> {
        let kind = kind.to_string();
        let text = read_strings(env, &text)?;
        match kind.as_str() {
            "intent" => super::intent::arrived(),
            "window" => super::system::window_changed(&text),
            "gamepad" => super::system::gamepad_moved(&text),
            _ => {}
        }
        Ok(())
    })
    .resolve::<jni::errors::LogErrorAndDefault>();
}
