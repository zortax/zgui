//! Executes the finished scene in its planned drawing order.

use std::collections::BTreeSet;

use wasm_bindgen::JsValue;
use web_sys::{CanvasWindingRule, Path2d};
use zgui_bits::DamageSet;
use zgui_geom::Matrix4;
use zgui_scene::{
    Batch, ClipId, ClipLink, ColorSprite, Decoration, GroupBoundary, MarkItem, MarkPayload, Paint,
    PaintRef, PrimitiveKind, Quad, Scene, Shadow, SpriteTile, VectorItem,
};

use crate::{Surface, atlas::CanvasAtlas, paint, path};

pub(crate) fn draw(
    surface: Surface,
    scene: &Scene,
    damage: &DamageSet,
    atlas: &mut CanvasAtlas,
    warnings: &mut BTreeSet<&'static str>,
) -> Result<u32, JsValue> {
    surface.context.reset_transform()?;
    surface.context.set_global_alpha(1.0);
    surface
        .context
        .set_global_composite_operation("source-over")?;
    surface.context.set_filter("none");
    if !damage.is_full() {
        surface.context.begin_path();
        for rect in damage.rects() {
            surface.context.rect(
                rect.origin.x as f64,
                rect.origin.y as f64,
                rect.size.width as f64,
                rect.size.height as f64,
            );
        }
        surface.context.clip();
    }
    surface.context.clear_rect(
        0.0,
        0.0,
        surface.canvas.width() as f64,
        surface.canvas.height() as f64,
    );
    let matrices = scene.placements().matrices().collect();
    let mut frame = Frame {
        surface,
        scene,
        matrices,
        atlas,
        warnings,
        groups: Vec::new(),
        calls: 0,
    };
    for batch in scene.batches() {
        frame.batch(batch)?;
    }
    if !frame.groups.is_empty() {
        return Err(JsValue::from_str("unclosed scene group"));
    }
    Ok(frame.calls)
}

struct Frame<'a> {
    surface: Surface,
    scene: &'a Scene,
    matrices: Vec<Matrix4>,
    atlas: &'a mut CanvasAtlas,
    warnings: &'a mut BTreeSet<&'static str>,
    groups: Vec<(Surface, GroupBoundary)>,
    calls: u32,
}

impl Frame<'_> {
    fn batch(&mut self, batch: Batch) -> Result<(), JsValue> {
        match batch {
            Batch::Quads(range) => {
                for position in range {
                    let quad =
                        self.scene.primitives.quads[self.slot(PrimitiveKind::Quad, position)];
                    self.with_state(ClipId(quad.clip), quad.transform, |frame| frame.quad(&quad))?;
                }
            }
            Batch::Shadows(range) => {
                for position in range {
                    let shadow =
                        self.scene.primitives.shadows[self.slot(PrimitiveKind::Shadow, position)];
                    self.with_state(ClipId(shadow.clip), shadow.transform, |frame| {
                        frame.shadow(&shadow)
                    })?;
                }
            }
            Batch::Decorations(range) => {
                for position in range {
                    let decoration = self.scene.primitives.decorations
                        [self.slot(PrimitiveKind::Decoration, position)];
                    self.with_state(ClipId(decoration.clip), decoration.transform, |frame| {
                        frame.decoration(&decoration)
                    })?;
                }
            }
            Batch::MonoSprites { range, .. } => {
                for position in range {
                    let sprite = self.scene.primitives.mono_sprites
                        [self.slot(PrimitiveKind::MonoSprite, position)];
                    self.with_state(ClipId(sprite.clip), sprite.transform, |frame| {
                        frame.sprite(sprite.tile, sprite.bounds, Some(sprite.color))
                    })?;
                }
            }
            Batch::SubpixelSprites { range, .. } => {
                for position in range {
                    let sprite = self.scene.primitives.subpixel_sprites
                        [self.slot(PrimitiveKind::SubpixelSprite, position)];
                    self.with_state(ClipId(sprite.clip), sprite.transform, |frame| {
                        frame.sprite(sprite.tile, sprite.bounds, Some(sprite.color))
                    })?;
                }
            }
            Batch::ColorSprites { range, .. } => {
                for position in range {
                    let sprite = self.scene.primitives.color_sprites
                        [self.slot(PrimitiveKind::ColorSprite, position)];
                    self.with_state(ClipId(sprite.clip), sprite.transform, |frame| {
                        frame.color_sprite(&sprite)
                    })?;
                }
            }
            Batch::Vector(index) => {
                let items: Vec<_> = self
                    .scene
                    .pass_plan()
                    .items_of(&self.scene.pass_plan().passes[index])
                    .iter()
                    .map(|planned| planned.item)
                    .collect();
                for index in items {
                    let item = &self.scene.primitives.vectors[index];
                    self.with_state(
                        item.clip,
                        item.transform.map_or(0, |id| id.index()),
                        |frame| frame.vector(item),
                    )?;
                }
            }
            Batch::Marks(range) => {
                for position in range {
                    let slot = self.slot(PrimitiveKind::Marks, position);
                    let item = self.scene.primitives.marks[slot];
                    let payload = self.scene.primitives.mark_payloads[slot].clone();
                    self.with_state(ClipId(item.clip), item.transform, |frame| {
                        frame.marks(&item, &payload)
                    })?;
                }
            }
            Batch::Group(index) => self.group(&self.scene.primitives.groups[index])?,
            Batch::External(_) => {
                paint::warn(self.warnings, "external GPU surfaces are unsupported")
            }
            Batch::Backdrop(_) => paint::warn(self.warnings, "backdrop filters are unsupported"),
            Batch::Shaded { .. } => {
                paint::warn(self.warnings, "custom shader primitives are unsupported")
            }
        }
        Ok(())
    }

    fn slot(&self, kind: PrimitiveKind, position: usize) -> usize {
        self.scene.remap(kind)[position] as usize
    }

    fn with_state(
        &mut self,
        clip: ClipId,
        transform: u32,
        draw: impl FnOnce(&mut Self) -> Result<(), JsValue>,
    ) -> Result<(), JsValue> {
        let context = self.surface.context.clone();
        context.save();
        let result = (|| {
            self.clip(clip)?;
            self.transform(transform)?;
            draw(self)
        })();
        context.restore();
        result
    }

    fn transform(&mut self, slot: u32) -> Result<(), JsValue> {
        let matrix = self
            .matrices
            .get(slot as usize)
            .copied()
            .unwrap_or(Matrix4::IDENTITY);
        if !matrix.is_2d() {
            paint::warn(
                self.warnings,
                "3D transforms use their 2D affine components",
            );
        }
        path::matrix(&self.surface.context, matrix)
    }

    fn clip(&mut self, id: ClipId) -> Result<(), JsValue> {
        for link in self.scene.clips.links(id) {
            match link {
                ClipLink::RoundedRect {
                    rect,
                    radii,
                    shape,
                    space,
                } => {
                    self.transform(space.index())?;
                    if shape.get() != 2.0 {
                        paint::warn(self.warnings, "superellipse corners use elliptical curves");
                    }
                    let bez = zgui_scene::clip::path::rounded_rect(rect, radii);
                    self.surface
                        .context
                        .clip_with_path_2d(&path::from_bez(&bez)?);
                }
                ClipLink::Mask { .. } => {
                    // Keep unsupported masks closed so clipped content cannot leak out.
                    paint::warn(
                        self.warnings,
                        "atlas mask clips are unsupported; masked content is hidden",
                    );
                    self.surface.context.begin_path();
                    self.surface.context.rect(0.0, 0.0, 0.0, 0.0);
                    self.surface.context.clip();
                }
            }
        }
        Ok(())
    }

    fn fill(
        &mut self,
        paint: PaintRef,
        origin: [f32; 2],
        brush: kurbo::Affine,
    ) -> Result<bool, JsValue> {
        paint::set_fill(
            &self.surface.context,
            self.scene,
            paint,
            origin,
            brush,
            self.warnings,
        )
    }

    fn quad(&mut self, quad: &Quad) -> Result<(), JsValue> {
        if quad.shape != 2.0 {
            paint::warn(self.warnings, "superellipse corners use elliptical curves");
        }
        let outer = path::rounded(quad.bounds, quad.radii);
        let outline = path::from_bez(&outer)?;
        if self.fill(quad.fill, quad.paint_origin, kurbo::Affine::IDENTITY)? {
            self.surface.context.fill_with_path_2d(&outline);
            self.calls += 1;
        }
        if quad.border.iter().all(|width| *width <= 0.0) || quad.stroke.is_none() {
            return Ok(());
        }
        if quad.style & 0xff != 0 {
            paint::warn(
                self.warnings,
                "dashed and dotted borders use a solid border",
            );
        }
        let (bounds, radii) = path::inset(quad.bounds, quad.radii, quad.border);
        if bounds[2] > 0.0 && bounds[3] > 0.0 {
            path::append(&outline, &path::rounded(bounds, radii).reverse_subpaths());
        }
        let sides = quad
            .stroke
            .id()
            .and_then(|id| self.scene.paints.get(id))
            .and_then(|paint| {
                if let Paint::Sides(colors) = paint {
                    Some(*colors)
                } else {
                    None
                }
            });
        if let Some(colors) = sides {
            let [x, y, w, h] = quad.bounds.map(f64::from);
            let corners = [(x, y), (x + w, y), (x + w, y + h), (x, y + h)];
            let [top, right, bottom, left] = quad.border.map(f64::from);
            let inside = [
                (x + left, y + top),
                (x + w - right, y + top),
                (x + w - right, y + h - bottom),
                (x + left, y + h - bottom),
            ];
            for (side, color) in colors.into_iter().enumerate() {
                let context = &self.surface.context;
                context.save();
                context.begin_path();
                context.move_to(corners[side].0, corners[side].1);
                context.line_to(corners[(side + 1) % 4].0, corners[(side + 1) % 4].1);
                context.line_to(inside[(side + 1) % 4].0, inside[(side + 1) % 4].1);
                context.line_to(inside[side].0, inside[side].1);
                context.close_path();
                context.clip();
                context.set_fill_style_str(&paint::color(color));
                context.fill_with_path_2d(&outline);
                context.restore();
                self.calls += 1;
            }
        } else if self.fill(quad.stroke, quad.paint_origin, kurbo::Affine::IDENTITY)? {
            self.surface.context.fill_with_path_2d(&outline);
            self.calls += 1;
        }
        Ok(())
    }

    fn sprite(
        &mut self,
        tile: SpriteTile,
        bounds: [f32; 4],
        tint: Option<[f32; 4]>,
    ) -> Result<(), JsValue> {
        if bounds[2] <= 0.0 || bounds[3] <= 0.0 || tile.bounds[2] <= 0 || tile.bounds[3] <= 0 {
            return Ok(());
        }
        let image = self.atlas.image(tile, tint)?;
        let [x, y, width, height] = bounds.map(f64::from);
        self.surface
            .context
            .set_image_smoothing_enabled(tile.texture >> 16 == 3);
        self.surface
            .context
            .draw_image_with_html_canvas_element_and_dw_and_dh(&image, x, y, width, height)?;
        self.calls += 1;
        Ok(())
    }

    fn color_sprite(&mut self, sprite: &ColorSprite) -> Result<(), JsValue> {
        let outline = path::from_bez(&path::rounded(sprite.frame, sprite.radii))?;
        self.surface.context.clip_with_path_2d(&outline);
        self.surface.context.set_global_alpha(sprite.opacity as f64);
        if sprite.flags & ColorSprite::GRAYSCALE != 0 {
            self.surface.context.set_filter("grayscale(1)");
        }
        self.sprite(sprite.tile, sprite.bounds, None)
    }

    fn vector(&mut self, item: &VectorItem) -> Result<(), JsValue> {
        path::affine(&self.surface.context, item.placement)?;
        for clip in &item.clips {
            self.surface
                .context
                .clip_with_path_2d_and_winding(&path::from_bez(&clip.path)?, path::rule(clip.rule));
        }
        if let Some(fill) = item.fill
            && self.fill(fill, [0.0; 2], item.brush)?
        {
            self.surface.context.fill_with_path_2d_and_winding(
                &path::from_bez(&item.path)?,
                path::rule(item.fill_rule),
            );
            self.calls += 1;
        }
        if let Some(stroke) = &item.stroke
            && self.fill(stroke.paint, [0.0; 2], item.brush)?
        {
            let outline = kurbo::stroke(
                item.path.elements().iter().copied(),
                &stroke.style,
                &kurbo::StrokeOpts::default(),
                0.1,
            );
            self.surface
                .context
                .fill_with_path_2d(&path::from_bez(&outline)?);
            self.calls += 1;
        }
        Ok(())
    }

    fn shadow(&mut self, shadow: &Shadow) -> Result<(), JsValue> {
        if shadow.inset != 0 {
            paint::warn(self.warnings, "inset box shadows are unsupported");
            return Ok(());
        }
        let outside = Path2d::new()?;
        outside.rect(-1e7, -1e7, 2e7, 2e7);
        path::append(
            &outside,
            &path::rounded(shadow.element_bounds, shadow.element_radii),
        );
        self.surface
            .context
            .clip_with_path_2d_and_winding(&outside, CanvasWindingRule::Evenodd);
        self.surface
            .context
            .set_fill_style_str(&paint::premultiplied(shadow.color));
        self.surface
            .context
            .set_filter(&format!("blur({}px)", shadow.blur.max(0.0)));
        self.surface
            .context
            .fill_with_path_2d(&path::from_bez(&path::rounded(
                shadow.shape_bounds,
                shadow.radii,
            ))?);
        self.calls += 1;
        Ok(())
    }

    fn decoration(&mut self, item: &Decoration) -> Result<(), JsValue> {
        let [x, y, width, height] = item.bounds.map(f64::from);
        let thickness = f64::from(item.thickness.max(0.5));
        let context = &self.surface.context;
        context.set_fill_style_str(&paint::premultiplied(item.color));
        match item.style {
            0 => context.fill_rect(x, y, width, height),
            4 => {
                context.fill_rect(x, y, width, thickness);
                context.fill_rect(x, y + height - thickness, width, thickness);
            }
            _ => {
                paint::warn(
                    self.warnings,
                    "wavy, dashed and dotted text decorations use a solid line",
                );
                context.fill_rect(x, y, width, thickness);
            }
        }
        self.calls += 1;
        Ok(())
    }

    fn marks(&mut self, item: &MarkItem, payload: &MarkPayload) -> Result<(), JsValue> {
        if item.flags & zgui_scene::MarkFlags::SCREEN != 0 {
            paint::warn(self.warnings, "screen-sized marks use local-space sizes");
        }
        let [a, b, c, d] = item.axes.map(f64::from);
        self.surface
            .context
            .transform(a, b, c, d, item.origin[0] as f64, item.origin[1] as f64)?;
        if !self.fill(item.paint, item.paint_origin, kurbo::Affine::IDENTITY)? {
            return Ok(());
        }
        let outline = Path2d::new()?;
        let first = item.first as usize;
        for [cx, cy, outer, inner] in payload
            .discs
            .iter()
            .skip(first)
            .take(item.discs as usize)
            .copied()
        {
            if item.flags & zgui_scene::MarkFlags::SQUARE_DISCS != 0 {
                let [cx, cy, outer, inner] = [cx, cy, outer.max(0.0), inner].map(f64::from);
                outline.rect(cx - outer, cy - outer, 2.0 * outer, 2.0 * outer);
                if inner > 0.0 {
                    // Reverse the inner contour so nonzero fill leaves the square hole open.
                    outline.move_to(cx - inner, cy - inner);
                    outline.line_to(cx - inner, cy + inner);
                    outline.line_to(cx + inner, cy + inner);
                    outline.line_to(cx + inner, cy - inner);
                    outline.close_path();
                }
                continue;
            }
            outline.move_to((cx + outer) as f64, cy as f64);
            outline.arc(
                cx as f64,
                cy as f64,
                outer.max(0.0) as f64,
                0.0,
                std::f64::consts::TAU,
            )?;
            outline.close_path();
            if inner > 0.0 {
                outline.move_to((cx + inner) as f64, cy as f64);
                outline.arc_with_anticlockwise(
                    cx as f64,
                    cy as f64,
                    inner as f64,
                    0.0,
                    std::f64::consts::TAU,
                    true,
                )?;
                outline.close_path();
            }
        }
        for rect in payload.boxes.iter().skip(first).take(item.boxes as usize) {
            if rect.shape[0] != 2.0 {
                paint::warn(self.warnings, "superellipse corners use elliptical curves");
            }
            let bounds = [
                rect.rect[0],
                rect.rect[1],
                rect.rect[2] - rect.rect[0],
                rect.rect[3] - rect.rect[1],
            ];
            path::append(&outline, &path::rounded(bounds, rect.radii));
            if rect.shape[1] > 0.0 {
                let (bounds, radii) = path::inset(bounds, rect.radii, [rect.shape[1]; 4]);
                path::append(&outline, &path::rounded(bounds, radii).reverse_subpaths());
            }
        }
        let mut line = kurbo::BezPath::new();
        let mut open = false;
        for [x, y] in payload
            .vertices
            .iter()
            .skip(first)
            .take(item.vertices as usize)
            .copied()
        {
            if !x.is_finite() || !y.is_finite() {
                open = false;
                continue;
            }
            if open {
                line.line_to((x as f64, y as f64));
            } else {
                line.move_to((x as f64, y as f64));
                open = true;
            }
        }
        if !line.elements().is_empty() {
            let cap = |bits| match bits {
                1 => kurbo::Cap::Square,
                2 => kurbo::Cap::Round,
                _ => kurbo::Cap::Butt,
            };
            let mut style =
                kurbo::Stroke::new(item.half_width as f64 * 2.0).with_join(kurbo::Join::Round);
            style.start_cap = cap((item.flags >> zgui_scene::MarkFlags::START_CAP_SHIFT) & 3);
            style.end_cap = cap((item.flags >> zgui_scene::MarkFlags::END_CAP_SHIFT) & 3);
            path::append(
                &outline,
                &kurbo::stroke(
                    line.elements().iter().copied(),
                    &style,
                    &kurbo::StrokeOpts::default(),
                    0.1,
                ),
            );
        }
        self.surface.context.fill_with_path_2d(&outline);
        self.calls += 1;
        if item.glyphs > 0 {
            paint::warn(
                self.warnings,
                "instanced vector glyph sheets are unsupported",
            );
        }
        Ok(())
    }

    fn group(&mut self, group: &GroupBoundary) -> Result<(), JsValue> {
        if group.is_start {
            let surface = Surface::new(self.surface.canvas.width(), self.surface.canvas.height())?;
            let parent = std::mem::replace(&mut self.surface, surface);
            self.groups.push((parent, group.clone()));
        } else {
            let (parent, opening) = self
                .groups
                .pop()
                .ok_or_else(|| JsValue::from_str("unmatched group end"))?;
            let child = std::mem::replace(&mut self.surface, parent);
            self.with_state(opening.clip, 0, |frame| {
                frame
                    .surface
                    .context
                    .set_global_alpha(opening.opacity.clamp(0.0, 1.0) as f64);
                frame
                    .surface
                    .context
                    .set_global_composite_operation(paint::blend(opening.blend))?;
                frame
                    .surface
                    .context
                    .set_filter(&paint::filters(&opening.filters, frame.warnings));
                frame.surface.context.draw_image_with_html_canvas_element(
                    &child.canvas,
                    0.0,
                    0.0,
                )?;
                frame.calls += 1;
                Ok(())
            })?;
        }
        Ok(())
    }
}
