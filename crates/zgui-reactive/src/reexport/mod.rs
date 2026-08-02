//! The reactive vocabulary: signals, memos, properties, callbacks, effects and async values.
//!
//! Everything here is re-exported from the crate root, which is where callers should name it.
//! The modules exist so that each type is documented beside the ones it is chosen against.
//!
//! Two things are deliberately missing. The engine's thread-safe effect constructors are not
//! published, because an effect that runs off the UI thread cannot touch the document;
//! [`RenderEffect`] is the one to use. And the engine's own selector is wrapped rather than
//! re-exported, because it never evicts — see [`Selector`].

mod async_;
mod callback;
mod effect;
mod memo;
mod selector;
mod signal;
mod storage;
mod wrappers;

pub use async_::{Action, ArcAction, ArcAsyncDerived, AsyncDerived, AsyncTransition};
pub use callback::{Callable, Callback, UnsyncCallback};
pub use effect::RenderEffect;
pub use memo::{ArcMemo, Memo, create_slice};
pub use selector::Selector;
pub use signal::{
    ArcReadSignal, ArcRwSignal, ArcTrigger, ArcWriteSignal, ReadSignal, RwSignal, Trigger,
    WriteSignal, arc_signal, signal, signal_local,
};
pub use storage::{LocalStorage, Storage, SyncStorage};
pub use wrappers::{ArcSignal, MaybeProp, Signal, SignalSetter};
