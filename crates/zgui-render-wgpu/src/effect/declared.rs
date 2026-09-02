//! Every effect the process has declared, so each device can register the ones it has not seen.
//!
//! # Why a list and not a call
//!
//! An application declares its effects while it is starting, which is before any window exists and
//! therefore before any device does. A second window on a second device has to end up with the
//! same effects as the first, and a device lost and rebuilt has to end up with them again. All
//! three are the same requirement: the declarations outlive every device, and a device catches up
//! with them rather than being told about them.
//!
//! The list only grows. An effect is a `static` in the application's binary, so there is nothing to
//! free, and a bounded list of a handful of entries is not worth a reclamation scheme.

use std::sync::{OnceLock, RwLock};

use zgui_scene::ShaderId;

use crate::effect::EffectProgram;

/// The declarations, and how many times the list has changed.
struct Declarations {
    /// Each declared effect, by the handle it was declared under.
    entries: Vec<(ShaderId, EffectProgram)>,
}

/// The process's declarations.
fn declarations() -> &'static RwLock<Declarations> {
    static DECLARED: OnceLock<RwLock<Declarations>> = OnceLock::new();
    DECLARED.get_or_init(|| {
        RwLock::new(Declarations {
            entries: Vec::new(),
        })
    })
}

/// Declares `program` under `id`, which is the handle the effect was declared to the scene under.
///
/// The handle is minted where the *declaration* is — see
/// [`zgui_scene::declare_shader`] — so one handle names both halves of an effect: the vocabulary
/// the paint stage decides with, and the program the device compiles.
pub fn declare(id: ShaderId, program: EffectProgram) {
    if let Ok(mut held) = declarations().write() {
        held.entries.push((id, program));
    }
}

/// How many effects the process has declared.
///
/// A device that has registered this many has registered all of them, which is the comparison that
/// makes catching up cost one atomic read on the overwhelming majority of frames.
pub fn count() -> usize {
    declarations().read().map_or(0, |held| held.entries.len())
}

/// Runs `with` over every declaration past the first `from`, in declaration order.
pub fn since(from: usize, mut with: impl FnMut(ShaderId, EffectProgram)) {
    let Ok(held) = declarations().read() else {
        return;
    };
    for (id, program) in held.entries.iter().skip(from) {
        with(*id, *program);
    }
}

#[cfg(test)]
mod tests {
    use super::{count, declare, since};
    use crate::effect::EffectProgram;

    #[test]
    fn a_declaration_is_visible_to_a_device_that_has_not_caught_up() {
        let before = count();
        let id = zgui_scene::declare_shader(
            "wgpu-declared-test",
            zgui_scene::ShaderMode::Paint,
            zgui_scene::ShaderReads::NOTHING,
            &[],
            0.0,
        );
        declare(id, EffectProgram::EMPTY);
        assert_eq!(count(), before + 1);
        let mut seen = Vec::new();
        since(before, |handle, _| seen.push(handle));
        assert_eq!(seen, vec![id]);
    }
}
