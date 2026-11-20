//! What an effect is drawn with, and how an application changes it.

use compositor_types::ParamsBlock as Block;
use zgui_paint::content::shader::{MAX_PARAMS_BYTES, ShaderId, ShaderReads, SharedParams};

use crate::program::ParamsLayout;

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

/// A registered effect, and the parameters it draws with.
///
/// Cloning shares the parameters: two clones are two names for one effect, and setting the
/// parameters through either is seen through both. A change is drawn on the next frame of every
/// window that draws the effect, without the element that paints it painting again.
#[derive(Debug)]
pub struct ShaderHandle<P> {
    /// The handle the effect was declared under.
    id: ShaderId,
    /// What the effect reads that changes on its own.
    reads: ShaderReads,
    /// The parameters, shared between clones and with the windows that draw them.
    params: SharedParams,
    /// The parameter structure this handle is written with.
    marker: core::marker::PhantomData<fn(P)>,
}

impl<P> Clone for ShaderHandle<P> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            reads: self.reads,
            params: self.params.clone(),
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
            params: SharedParams::new(),
            marker: core::marker::PhantomData,
        }
    }

    /// Draws with `params` from now on.
    ///
    /// The pointer set with [`set_pointer`](ShaderHandle::set_pointer) is kept.
    pub fn set_params(&self, params: P) {
        self.params.update(|block| {
            block.user = [0; MAX_PARAMS_BYTES];
            params.write(&mut block.user);
        });
    }

    /// Tells the effect where the pointer is, in the element's own device pixels.
    ///
    /// Only useful to an effect that declared it reads the pointer; a handle whose effect did not
    /// is drawn identically whatever this says.
    pub fn set_pointer(&self, pointer: [f32; 2], hovered: bool) {
        self.params.update(|block| {
            block.pointer = pointer;
            block.hovered = if hovered { 1.0 } else { 0.0 };
        });
    }

    /// The handle the effect was declared under, which items draw it by.
    pub fn id(&self) -> ShaderId {
        self.id
    }

    /// What the effect reads that changes on its own.
    pub fn reads(&self) -> ShaderReads {
        self.reads
    }

    /// The block the effect is currently drawn with.
    pub fn params(&self) -> Block {
        self.params.block()
    }

    /// The block shared with the windows that draw the effect.
    pub fn shared(&self) -> &SharedParams {
        &self.params
    }

    /// How many times the parameters have changed.
    pub fn revision(&self) -> u64 {
        self.params.revision()
    }
}

#[cfg(test)]
mod tests {
    use super::{NoParams, ShaderHandle, ShaderParams};
    use crate::program::{ParamsField, ParamsLayout};
    use zgui_paint::content::shader::{MAX_PARAMS_BYTES, ShaderId, ShaderReads};

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

        fn write(&self, out: &mut [u8; MAX_PARAMS_BYTES]) {
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
        let handle: ShaderHandle<NoParams> = ShaderHandle::new(ShaderId(2), ShaderReads::NOTHING);
        handle.set_params(NoParams);
        assert_eq!(handle.params().user, [0u8; MAX_PARAMS_BYTES]);
    }
}
