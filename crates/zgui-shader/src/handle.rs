//! What an effect is drawn with, and how an application changes it.

use std::sync::{Arc, Mutex};

use zgui_render_wgpu::ParamsLayout;
use zgui_scene::{MAX_PARAMS_BYTES, ShaderId, ShaderParams as Block};
use zgui_wgsl::ShaderReads;

/// A structure an effect's `Params` is written as.
///
/// Derive it with [`ShaderParams`](derive@crate::ShaderParams), which reads the layout out of the
/// compiler and writes each field where the layout says it goes. Writing an implementation by hand
/// defeats the comparison the layout exists for.
pub trait ShaderParams: Copy {
    /// The layout, as Rust has it.
    const LAYOUT: ParamsLayout;

    /// Writes the structure into an effect's block.
    fn write(&self, out: &mut [u8; MAX_PARAMS_BYTES]);
}

/// A value a shader parameter may be.
///
/// A closed set on purpose: it is what WGSL can hold at these widths, and every one of them writes
/// the same bytes in Rust and in the shader. A parameter that is anything else is a parameter the
/// shader could not have declared.
pub trait ParamsValue: Copy {
    /// How many bytes it occupies.
    const BYTES: usize;

    /// Writes it into `out`, which is exactly [`ParamsValue::BYTES`] long.
    fn write(self, out: &mut [u8]);
}

/// Implements [`ParamsValue`] for a scalar and for the short arrays of it WGSL has vectors for.
macro_rules! params_value {
    ($($scalar:ty),+ $(,)?) => {
        $(
            impl ParamsValue for $scalar {
                const BYTES: usize = size_of::<$scalar>();

                fn write(self, out: &mut [u8]) {
                    out.copy_from_slice(&self.to_ne_bytes());
                }
            }

            impl<const N: usize> ParamsValue for [$scalar; N] {
                const BYTES: usize = N * size_of::<$scalar>();

                fn write(self, out: &mut [u8]) {
                    let width = size_of::<$scalar>();
                    for (index, value) in self.into_iter().enumerate() {
                        out[index * width..(index + 1) * width]
                            .copy_from_slice(&value.to_ne_bytes());
                    }
                }
            }
        )+
    };
}

params_value!(f32, u32, i32);

/// The parameters of an effect that declares none.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NoParams;

impl ShaderParams for NoParams {
    const LAYOUT: ParamsLayout = ParamsLayout::EMPTY;

    fn write(&self, _out: &mut [u8; MAX_PARAMS_BYTES]) {}
}

/// What one mounted effect is currently drawn with.
#[derive(Debug)]
struct Slot {
    /// The block, framework half and application half together.
    params: Block,
    /// How many times the block has changed, so a caller can tell whether it owes a repaint.
    revision: u64,
}

/// A registered effect, and the parameters it draws with.
///
/// Cloning shares the parameters: two clones are two names for one effect, and setting the
/// parameters through either is seen through both.
#[derive(Debug)]
pub struct ShaderHandle<P> {
    /// The handle a display list names the effect by.
    id: ShaderId,
    /// What the effect reads that changes on its own.
    reads: ShaderReads,
    /// The parameters, shared between clones.
    slot: Arc<Mutex<Slot>>,
    /// The parameter structure this handle is written with.
    marker: core::marker::PhantomData<fn(P)>,
}

impl<P> Clone for ShaderHandle<P> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            reads: self.reads,
            slot: Arc::clone(&self.slot),
            marker: core::marker::PhantomData,
        }
    }
}

impl<P: ShaderParams> ShaderHandle<P> {
    /// A handle on the effect `id` reads, drawing with zeroed parameters.
    pub(crate) fn new(id: ShaderId, reads: ShaderReads) -> Self {
        Self {
            id,
            reads,
            slot: Arc::new(Mutex::new(Slot {
                params: Block::EMPTY,
                revision: 0,
            })),
            marker: core::marker::PhantomData,
        }
    }

    /// Draws with `params` from now on.
    ///
    /// The pointer the framework fills in is kept: it is the framework's half of the block and an
    /// application does not write it.
    ///
    /// This changes what the *next* painting draws. An element that does not repaint keeps showing
    /// the parameters it was painted with, so a change here belongs beside whatever bumps the
    /// element's paint revision.
    pub fn set_params(&self, params: P) {
        self.update(|held| {
            held.params.user = [0; MAX_PARAMS_BYTES];
            params.write(&mut held.params.user);
        });
    }

    /// Tells the effect where the pointer is, in the element's own device pixels.
    ///
    /// Only useful to an effect that declared it reads the pointer; a handle whose effect did not
    /// is drawn identically whatever this says.
    pub fn set_pointer(&self, pointer: [f32; 2], hovered: bool) {
        self.update(|held| held.params = held.params.with_pointer(pointer, hovered));
    }

    /// The handle a display list names this effect by.
    pub fn id(&self) -> ShaderId {
        self.id
    }

    /// What the effect reads that changes on its own.
    pub fn reads(&self) -> ShaderReads {
        self.reads
    }

    /// The block the effect is currently drawn with.
    pub fn params(&self) -> Block {
        self.slot
            .lock()
            .map_or(Block::EMPTY, |held| held.params)
    }

    /// How many times the parameters have changed.
    ///
    /// Fold it into a custom element's paint revision, and the element repaints exactly when the
    /// effect would draw something different.
    pub fn revision(&self) -> u64 {
        self.slot.lock().map_or(0, |held| held.revision)
    }

    /// Applies `change` and counts it.
    fn update(&self, change: impl FnOnce(&mut Slot)) {
        if let Ok(mut held) = self.slot.lock() {
            change(&mut held);
            held.revision = held.revision.wrapping_add(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{NoParams, ShaderHandle, ShaderParams};
    use zgui_render_wgpu::{ParamsField, ParamsLayout};
    use zgui_scene::ShaderId;
    use zgui_wgsl::ShaderReads;

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct Two {
        first: f32,
        second: f32,
    }

    impl ShaderParams for Two {
        const LAYOUT: ParamsLayout = ParamsLayout {
            size: 8,
            fields: &[
                ParamsField {
                    name: "first",
                    offset: 0,
                    size: 4,
                },
                ParamsField {
                    name: "second",
                    offset: 4,
                    size: 4,
                },
            ],
        };

        fn write(&self, out: &mut [u8; zgui_scene::MAX_PARAMS_BYTES]) {
            super::ParamsValue::write(self.first, &mut out[0..4]);
            super::ParamsValue::write(self.second, &mut out[4..8]);
        }
    }

    fn handle() -> ShaderHandle<Two> {
        ShaderHandle::new(ShaderId(1), ShaderReads::NOTHING)
    }

    #[test]
    fn parameters_reach_the_block_as_their_own_bytes() {
        let handle = handle();
        handle.set_params(Two {
            first: 1.5,
            second: -2.0,
        });
        let params = handle.params();
        assert_eq!(&params.user[0..4], &1.5f32.to_ne_bytes());
        assert_eq!(&params.user[4..8], &(-2.0f32).to_ne_bytes());
    }

    #[test]
    fn the_framework_half_survives_a_write_of_the_application_half() {
        let handle = handle();
        handle.set_pointer([4.0, 5.0], true);
        handle.set_params(Two {
            first: 1.0,
            second: 2.0,
        });
        let params = handle.params();
        assert_eq!(params.pointer, [4.0, 5.0]);
        assert_eq!(params.hovered, 1.0);
    }

    #[test]
    fn every_change_is_counted_so_a_caller_can_tell_it_owes_a_repaint() {
        let handle = handle();
        assert_eq!(handle.revision(), 0);
        handle.set_params(Two::default());
        assert_eq!(handle.revision(), 1);
        handle.set_pointer([0.0, 0.0], false);
        assert_eq!(handle.revision(), 2);
    }

    #[test]
    fn a_clone_is_a_second_name_for_one_effect() {
        let handle = handle();
        let second = handle.clone();
        second.set_params(Two {
            first: 9.0,
            second: 0.0,
        });
        assert_eq!(handle.revision(), 1);
        assert_eq!(&handle.params().user[0..4], &9.0f32.to_ne_bytes());
    }

    #[test]
    fn an_effect_with_no_parameters_still_has_a_block() {
        let handle: ShaderHandle<NoParams> =
            ShaderHandle::new(ShaderId(2), ShaderReads::NOTHING);
        handle.set_params(NoParams);
        assert_eq!(handle.params().user, [0u8; zgui_scene::MAX_PARAMS_BYTES]);
    }
}
