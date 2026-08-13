// DERIVED-FROM: the taffy project, src/compute/leaf.rs (MIT)
// The leaf sizing sequence below — percentage resolution against the inline size, the `box-sizing`
// adjustment, the min/max clamp, the transposed scrollbar gutter, the available-space derivation
// and the aspect-ratio fix-up — is adapted from that work, which is licensed under the MIT License,
// and has been modified in exactly two ways: the measure step receives the tree by mutable
// reference, so that an atomic inline can run a nested layout of its own, and the baselines the
// measurement reports are carried out, which the original discards.

//! Sizing a box whose height comes from its content rather than from an algorithm.

use taffy::{
    AvailableSpace, BoxSizing, CollapsibleMarginSet, CoreStyle, LayoutInput, LayoutOutput,
    MaybeMath, MaybeResolve, NodeId, Overflow, Point, Position, ResolveOrZero, RunMode, Size,
    SizingMode,
};

use crate::key::from_node_id;
use crate::measure::MeasureContent;
use crate::tree::LayoutTree;

/// Sizes one leaf box, and reports what its content's baselines came out at.
pub(crate) fn compute<C: MeasureContent>(
    tree: &mut LayoutTree<'_, C>,
    node: NodeId,
    inputs: LayoutInput,
    block: Option<&mut taffy::BlockContext<'_>>,
) -> (LayoutOutput, Option<f32>) {
    let LayoutInput {
        known_dimensions,
        parent_size,
        available_space,
        sizing_mode,
        run_mode,
        ..
    } = inputs;

    // Everything up to the measurement is a pure function of the style and the inputs, so it runs
    // under a shared borrow that is released before the tree is used exclusively below.
    let prepared = {
        let style = tree.style_of(from_node_id(node));
        let calc = |value: *const (), basis: f32| tree.resolve_calc(value, basis);

        // Both axes resolve percentage padding and border against the containing block's *inline*
        // size. That is not an oversight in CSS; it is what the specification says.
        let margin = style.margin().resolve_or_zero(parent_size.width, calc);
        let padding = style.padding().resolve_or_zero(parent_size.width, calc);
        let border = style.border().resolve_or_zero(parent_size.width, calc);
        let padding_border = padding + border;
        let pb_sum = padding_border.sum_axes();
        let box_sizing_adjustment = if style.box_sizing() == BoxSizing::ContentBox {
            pb_sum
        } else {
            Size::ZERO
        };

        let (node_size, node_min_size, node_max_size, aspect_ratio) = match sizing_mode {
            SizingMode::ContentSize => (known_dimensions, Size::NONE, Size::NONE, None),
            SizingMode::InherentSize => {
                let aspect_ratio = style.aspect_ratio();
                let style_size = style
                    .size()
                    .maybe_resolve(parent_size, calc)
                    .maybe_apply_aspect_ratio(aspect_ratio)
                    .maybe_add(box_sizing_adjustment);
                let style_min_size = style
                    .min_size()
                    .maybe_resolve(parent_size, calc)
                    .maybe_apply_aspect_ratio(aspect_ratio)
                    .maybe_add(box_sizing_adjustment);
                let style_max_size = style
                    .max_size()
                    .maybe_resolve(parent_size, calc)
                    .maybe_add(box_sizing_adjustment);
                (
                    known_dimensions.or(style_size),
                    style_min_size,
                    style_max_size,
                    aspect_ratio,
                )
            }
        };

        // The axes are transposed: a box that scrolls vertically needs *horizontal* space for the
        // scrollbar.
        let scrollbar_gutter = style.overflow().transpose().map(|overflow| match overflow {
            Overflow::Scroll => style.scrollbar_width(),
            _ => 0.0,
        });
        let mut content_box_inset = padding_border;
        content_box_inset.right += scrollbar_gutter.x;
        content_box_inset.bottom += scrollbar_gutter.y;

        let cannot_collapse_through = !style.is_block()
            || style.overflow().x.is_scroll_container()
            || style.overflow().y.is_scroll_container()
            || style.position() == Position::Absolute
            || padding.top > 0.0
            || padding.bottom > 0.0
            || border.top > 0.0
            || border.bottom > 0.0
            || matches!(node_size.height, Some(height) if height > 0.0)
            || matches!(node_min_size.height, Some(height) if height > 0.0);

        Prepared {
            padding,
            padding_border,
            content_box_inset,
            margin,
            node_size,
            node_min_size,
            node_max_size,
            aspect_ratio,
            cannot_collapse_through,
        }
    };

    // Both dimensions known and nothing to report: the content is never asked.
    if let (
        RunMode::ComputeSize,
        true,
        Size {
            width: Some(width),
            height: Some(height),
        },
    ) = (
        run_mode,
        prepared.cannot_collapse_through,
        prepared.node_size,
    ) {
        {
            let size = Size { width, height }
                .maybe_clamp(prepared.node_min_size, prepared.node_max_size)
                .maybe_max(prepared.padding_border.sum_axes().map(Some));
            return (
                LayoutOutput {
                    size,
                    content_size: Size::ZERO,
                    first_baselines: Point::NONE,
                    top_margin: CollapsibleMarginSet::ZERO,
                    bottom_margin: CollapsibleMarginSet::ZERO,
                    margins_can_collapse_through: false,
                },
                None,
            );
        }
    }

    let measure_space = Size {
        width: known_dimensions
            .width
            .map(AvailableSpace::from)
            .unwrap_or(available_space.width)
            .maybe_sub(prepared.margin.horizontal_axis_sum())
            .maybe_set(known_dimensions.width)
            .maybe_set(prepared.node_size.width)
            .map_definite_value(|size| {
                size.maybe_clamp(prepared.node_min_size.width, prepared.node_max_size.width)
                    - prepared.content_box_inset.horizontal_axis_sum()
            }),
        height: known_dimensions
            .height
            .map(AvailableSpace::from)
            .unwrap_or(available_space.height)
            .maybe_sub(prepared.margin.vertical_axis_sum())
            .maybe_set(known_dimensions.height)
            .maybe_set(prepared.node_size.height)
            .map_definite_value(|size| {
                size.maybe_clamp(prepared.node_min_size.height, prepared.node_max_size.height)
                    - prepared.content_box_inset.vertical_axis_sum()
            }),
    };

    // The one place this differs from the unforked arithmetic: the measurement takes the tree,
    // because an atomic inline's content is a nested layout of boxes this tree owns.
    let measured = crate::inline::measure_leaf(
        tree,
        node,
        match run_mode {
            RunMode::ComputeSize => known_dimensions,
            // A layout run passes no known dimensions, which is the signal to the content that the
            // lines it produces now are the ones that will be kept.
            RunMode::PerformLayout | RunMode::PerformHiddenLayout => Size::NONE,
        },
        measure_space,
        run_mode,
        inputs.axis,
        block,
    );
    let measured_size = measured.size;

    let clamped_size = known_dimensions
        .or(prepared.node_size)
        .unwrap_or(measured_size + prepared.content_box_inset.sum_axes())
        .maybe_clamp(prepared.node_min_size, prepared.node_max_size);
    let size = Size {
        width: clamped_size.width,
        height: clamped_size.height.max(
            prepared
                .aspect_ratio
                .map(|ratio| clamped_size.width / ratio)
                .unwrap_or(0.0),
        ),
    };
    let size = size.maybe_max(prepared.padding_border.sum_axes().map(Some));

    // A baseline the content reported is measured from the top of the *content* box, and every
    // consumer of it measures from the top of the border box.
    let offset = prepared.padding_border.top;
    let output = LayoutOutput {
        size,
        content_size: measured_size + prepared.padding.sum_axes(),
        first_baselines: Point {
            x: None,
            y: measured.first_baseline.map(|baseline| baseline + offset),
        },
        top_margin: CollapsibleMarginSet::ZERO,
        bottom_margin: CollapsibleMarginSet::ZERO,
        margins_can_collapse_through: !prepared.cannot_collapse_through
            && size.height == 0.0
            && measured_size.height == 0.0,
    };
    (
        output,
        measured.last_baseline.map(|baseline| baseline + offset),
    )
}

/// Everything the arithmetic derives from the style, computed before the tree is borrowed again.
struct Prepared {
    /// The resolved padding.
    padding: taffy::Rect<f32>,
    /// The resolved padding and border together.
    padding_border: taffy::Rect<f32>,
    /// The same, plus any scrollbar gutter.
    content_box_inset: taffy::Rect<f32>,
    /// The resolved margins.
    margin: taffy::Rect<f32>,
    /// The size the style asks for, with anything already known folded in.
    node_size: Size<Option<f32>>,
    /// The minimum the style asks for.
    node_min_size: Size<Option<f32>>,
    /// The maximum it asks for.
    node_max_size: Size<Option<f32>>,
    /// The proportions it should keep.
    aspect_ratio: Option<f32>,
    /// Whether this box's own styles stop its margins collapsing through it.
    cannot_collapse_through: bool,
}
