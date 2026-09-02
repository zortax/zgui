//! Turning the parameters a style sheet wrote into the block an effect is drawn with.

use zgui_geom::{Device, DevicePx, Point, Rect};
use zgui_scene::{Filter, Scene, ShaderParams};

use crate::content::shader::{ShaderBinding, ShaderSource};
use crate::lower::NamedShader;

/// The filter step `named` describes, or `None` when it names nothing that filters.
///
/// The reach is the effect's own declaration, in CSS pixels, scaled here — it is what makes the
/// damage a filtered box owes correct, and nothing but the effect can state it.
pub fn filter_step(
    scene: &mut Scene,
    named: Option<&NamedShader>,
    shaders: &dyn ShaderSource,
    scale: f32,
) -> Option<Filter> {
    let named = named?;
    let binding = shaders.effect(&named.name)?;
    if binding.mode != zgui_scene::ShaderMode::Filter {
        // The property said the effect filters and the effect does something else. Running it
        // anyway would read a target with a shader written to draw a rectangle.
        return None;
    }
    let params = scene.shader_params.intern(block(named, binding));
    Some(Filter::Custom {
        shader: binding.id,
        params,
        reach: binding.reach * scale,
    })
}

/// The block `named` describes, laid out the way `binding`'s effect declares it.
///
/// A property naming a field the effect does not declare is dropped, and a field the sheet did not
/// write keeps its zero. Both are what a style sheet and a shader that disagree should do: neither
/// is an error the cascade can report, and a rectangle full of a stranger's numbers is worse than
/// one full of zeroes.
pub fn block(named: &NamedShader, binding: ShaderBinding) -> ShaderParams {
    block_at(named, binding, None, Rect::ZERO)
}

/// The same block, telling an effect that declared it reads the pointer where the pointer is.
///
/// `box_` is the rectangle the effect is drawn over, because an effect is written against its own
/// coordinates and is told whether the pointer is over it. An effect that declared nothing is
/// handed a zero, which is what stops a pointer move repainting every box that carries one.
pub fn block_at(
    named: &NamedShader,
    binding: ShaderBinding,
    pointer: Option<Point<DevicePx, Device>>,
    box_: Rect<DevicePx, Device>,
) -> ShaderParams {
    let mut params = ShaderParams::EMPTY;
    if binding.reads.pointer
        && let Some(at) = pointer
    {
        let local = [at.x.0 - box_.origin.x.0, at.y.0 - box_.origin.y.0];
        params = params.with_pointer(local, box_.contains(at));
    }
    for (field, value) in &named.parameters {
        let Some(at) = binding.field(field) else {
            continue;
        };
        if at.size == size_of::<f32>() {
            params.user[at.offset..at.offset + at.size].copy_from_slice(&value.to_ne_bytes());
        }
    }
    params
}

#[cfg(test)]
mod tests {
    use super::{block, block_at};
    use crate::content::shader::ShaderBinding;
    use crate::lower::NamedShader;
    use zgui_geom::{DevicePx, Point, Rect, Size};
    use zgui_scene::ShaderField;
    use zgui_scene::{ShaderId, ShaderMode, ShaderReads};

    const FIELDS: [ShaderField; 2] = [
        ShaderField {
            name: "first",
            offset: 0,
            size: 4,
        },
        ShaderField {
            name: "second",
            offset: 4,
            size: 4,
        },
    ];

    fn binding() -> ShaderBinding {
        ShaderBinding {
            id: ShaderId(1),
            mode: ShaderMode::Coverage,
            reads: ShaderReads::NOTHING,
            fields: &FIELDS,
            reach: 0.0,
        }
    }

    fn named(parameters: &[(&str, f32)]) -> NamedShader {
        NamedShader {
            name: "test".to_owned(),
            mode: ShaderMode::Coverage,
            parameters: parameters
                .iter()
                .map(|(name, value)| ((*name).to_owned(), *value))
                .collect(),
        }
    }

    #[test]
    fn a_field_the_sheet_wrote_lands_where_the_effect_declares_it() {
        let params = block(&named(&[("second", 2.5)]), binding());
        assert_eq!(&params.user[4..8], &2.5f32.to_ne_bytes());
        assert_eq!(&params.user[0..4], &[0, 0, 0, 0], "the rest keeps its zero");
    }

    #[test]
    fn a_field_the_effect_does_not_declare_is_dropped() {
        let params = block(&named(&[("third", 9.0)]), binding());
        assert_eq!(params.user, [0u8; zgui_scene::MAX_PARAMS_BYTES]);
    }

    /// An effect that declared it reads the pointer is told where it is, in its own coordinates.
    #[test]
    fn the_pointer_reaches_an_effect_that_declared_it_in_the_box_s_own_coordinates() {
        let mut binding = binding();
        binding.reads = ShaderReads {
            pointer: true,
            time: false,
        };
        let box_ = Rect::new(
            Point::new(DevicePx(10.0), DevicePx(20.0)),
            Size::new(DevicePx(100.0), DevicePx(50.0)),
        );
        let at = Point::new(DevicePx(35.0), DevicePx(30.0));
        let params = block_at(&named(&[]), binding, Some(at), box_);
        assert_eq!(
            params.pointer,
            [25.0, 10.0],
            "measured from the box's corner"
        );
        assert_eq!(params.hovered, 1.0);
    }

    /// A box the pointer is outside of is told where it is and that it is not over it, so an effect
    /// can fade rather than jump.
    #[test]
    fn a_pointer_outside_the_box_is_reported_as_outside() {
        let mut binding = binding();
        binding.reads = ShaderReads {
            pointer: true,
            time: false,
        };
        let box_ = Rect::new(
            Point::new(DevicePx(0.0), DevicePx(0.0)),
            Size::new(DevicePx(10.0), DevicePx(10.0)),
        );
        let at = Point::new(DevicePx(40.0), DevicePx(5.0));
        let params = block_at(&named(&[]), binding, Some(at), box_);
        assert_eq!(params.pointer, [40.0, 5.0]);
        assert_eq!(params.hovered, 0.0);
    }

    /// An effect that declared nothing is drawn identically wherever the pointer is, which is what
    /// stops a pointer stream repainting every box that carries one.
    #[test]
    fn an_effect_that_declared_nothing_is_handed_no_pointer() {
        let box_ = Rect::new(
            Point::new(DevicePx(0.0), DevicePx(0.0)),
            Size::new(DevicePx(10.0), DevicePx(10.0)),
        );
        let at = Point::new(DevicePx(5.0), DevicePx(5.0));
        let params = block_at(&named(&[]), binding(), Some(at), box_);
        assert_eq!(params.pointer, [0.0, 0.0]);
        assert_eq!(params.hovered, 0.0);
    }

    #[test]
    fn every_field_the_sheet_wrote_reaches_the_block() {
        let params = block(&named(&[("first", 1.0), ("second", 2.0)]), binding());
        assert_eq!(&params.user[0..4], &1.0f32.to_ne_bytes());
        assert_eq!(&params.user[4..8], &2.0f32.to_ne_bytes());
    }
}
