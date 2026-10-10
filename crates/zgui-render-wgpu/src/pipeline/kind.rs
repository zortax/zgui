//! What each pipeline draws, and how.

use crate::shader::Module;

/// One drawing pipeline.
///
/// A pipeline is keyed by this *and* by the format of the attachment it draws into, because a
/// pipeline's colour target has to match the attachment or the draw is rejected. There are two
/// attachment formats in a frame — the composed target and an isolated group's — so most kinds
/// exist twice.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PipelineKind {
    /// Rounded, bordered rectangles.
    Quad,
    /// Box shadows.
    Shadow,
    /// Text decoration lines.
    Decoration,
    /// Single-channel coverage sprites.
    MonoSprite,
    /// Full-colour sprites.
    ColorSprite,
    /// Per-channel coverage sprites.
    SubpixelSprite,
    /// The copy from the composed target to the surface.
    Blit,
    /// The same copy, with the attachment's encode cancelled in advance.
    BlitUndoSrgb,
    /// Clearing one damage rectangle.
    DamageClear,
    /// The 2:1 downsample that begins a blur.
    BlurDownsample,
    /// One axis of a separable gaussian.
    BlurAxis,
    /// Compositing an isolated target back into the one beneath it.
    Composite,
    /// A rectangle showing a texture the renderer did not draw.
    External,
    /// Compositing a rasterised vector batch back into the target.
    VectorComposite,
    /// Painting the discs of a mark.
    MarksDisc,
    /// Adding the coverage of a mark's discs into its bin.
    MarksDiscCoverage,
    /// Painting the boxes of a mark.
    MarksBox,
    /// Adding the coverage of a mark's boxes into its bin.
    MarksBoxCoverage,
    /// Painting the polylines of a mark.
    MarksPolyline,
    /// Adding the coverage of a mark's polylines into its bin.
    MarksPolylineCoverage,
    /// Painting the glyphs of a mark.
    MarksGlyph,
    /// Adding the coverage of a mark's glyphs into its bin.
    MarksGlyphCoverage,
    /// Painting a union mark through the coverage in its bin.
    MarksComposite,
}

impl PipelineKind {
    /// Every kind.
    pub const ALL: [Self; 23] = [
        Self::Quad,
        Self::Shadow,
        Self::Decoration,
        Self::MonoSprite,
        Self::ColorSprite,
        Self::SubpixelSprite,
        Self::Blit,
        Self::BlitUndoSrgb,
        Self::DamageClear,
        Self::BlurDownsample,
        Self::BlurAxis,
        Self::Composite,
        Self::External,
        Self::VectorComposite,
        Self::MarksDisc,
        Self::MarksDiscCoverage,
        Self::MarksBox,
        Self::MarksBoxCoverage,
        Self::MarksPolyline,
        Self::MarksPolylineCoverage,
        Self::MarksGlyph,
        Self::MarksGlyphCoverage,
        Self::MarksComposite,
    ];

    /// The format a coverage draw writes: the bin pages, one quarter of a pixel in each channel.
    pub const COVERAGE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

    /// The pipeline drawing one payload kind of a mark, painting it or adding its coverage.
    pub fn marks(kind: crate::pipeline::marks::MarkKind, coverage: bool) -> Self {
        use crate::pipeline::marks::MarkKind;
        match (kind, coverage) {
            (MarkKind::Disc, false) => Self::MarksDisc,
            (MarkKind::Disc, true) => Self::MarksDiscCoverage,
            (MarkKind::Box, false) => Self::MarksBox,
            (MarkKind::Box, true) => Self::MarksBoxCoverage,
            (MarkKind::Polyline, false) => Self::MarksPolyline,
            (MarkKind::Polyline, true) => Self::MarksPolylineCoverage,
            (MarkKind::Glyph, false) => Self::MarksGlyph,
            (MarkKind::Glyph, true) => Self::MarksGlyphCoverage,
        }
    }

    /// Which shader module it is built from.
    pub fn module(self) -> Module {
        match self {
            Self::Quad => Module::Quad,
            Self::Shadow => Module::Shadow,
            Self::Decoration => Module::Decoration,
            Self::MonoSprite => Module::MonoSprite,
            Self::ColorSprite => Module::ColorSprite,
            Self::SubpixelSprite => Module::SubpixelSprite,
            Self::Blit | Self::BlitUndoSrgb => Module::Blit,
            Self::DamageClear => Module::Clear,
            Self::BlurDownsample | Self::BlurAxis => Module::Blur,
            Self::Composite => Module::Composite,
            Self::External => Module::External,
            Self::VectorComposite => Module::Vector,
            Self::MarksDisc | Self::MarksDiscCoverage => Module::MarksDisc,
            Self::MarksBox | Self::MarksBoxCoverage => Module::MarksBox,
            Self::MarksPolyline | Self::MarksPolylineCoverage => Module::MarksPolyline,
            Self::MarksGlyph | Self::MarksGlyphCoverage => Module::MarksGlyph,
            Self::MarksComposite => Module::MarksComposite,
        }
    }

    /// Its vertex entry point.
    pub fn vertex_entry(self) -> &'static str {
        match self {
            Self::Quad => "vs_quad",
            Self::Shadow => "vs_shadow",
            Self::Decoration => "vs_decoration",
            Self::MonoSprite => "vs_mono_sprite",
            Self::ColorSprite => "vs_color_sprite",
            Self::SubpixelSprite => "vs_subpixel_sprite",
            Self::Blit | Self::BlitUndoSrgb => "vs_blit",
            Self::DamageClear => "vs_clear",
            Self::BlurDownsample | Self::BlurAxis => "vs_blur",
            Self::Composite => "vs_composite",
            Self::External => "vs_external",
            Self::VectorComposite => "vs_vector",
            Self::MarksDisc => "vs_disc_paint",
            Self::MarksDiscCoverage => "vs_disc_coverage",
            Self::MarksBox => "vs_box_paint",
            Self::MarksBoxCoverage => "vs_box_coverage",
            Self::MarksPolyline => "vs_segment_paint",
            Self::MarksPolylineCoverage => "vs_segment_coverage",
            Self::MarksGlyph => "vs_glyph_paint",
            Self::MarksGlyphCoverage => "vs_glyph_coverage",
            Self::MarksComposite => "vs_mark_composite",
        }
    }

    /// Its fragment entry point.
    pub fn fragment_entry(self) -> &'static str {
        match self {
            Self::Quad => "fs_quad",
            Self::Shadow => "fs_shadow",
            Self::Decoration => "fs_decoration",
            Self::MonoSprite => "fs_mono_sprite",
            Self::ColorSprite => "fs_color_sprite",
            Self::SubpixelSprite => "fs_subpixel_sprite",
            Self::Blit => "fs_blit",
            Self::BlitUndoSrgb => "fs_blit_undo_srgb",
            Self::DamageClear => "fs_clear",
            Self::BlurDownsample => "fs_blur_downsample",
            Self::BlurAxis => "fs_blur_axis",
            Self::Composite => "fs_composite",
            Self::External => "fs_external",
            Self::VectorComposite => "fs_vector",
            Self::MarksDisc => "fs_disc_paint",
            Self::MarksDiscCoverage => "fs_disc_coverage",
            Self::MarksBox => "fs_box_paint",
            Self::MarksBoxCoverage => "fs_box_coverage",
            Self::MarksPolyline => "fs_segment_paint",
            Self::MarksPolylineCoverage => "fs_segment_coverage",
            Self::MarksGlyph => "fs_glyph_paint",
            Self::MarksGlyphCoverage => "fs_glyph_coverage",
            Self::MarksComposite => "fs_mark_composite",
        }
    }

    /// Whether it reads an atlas texture.
    pub fn samples_atlas(self) -> bool {
        matches!(
            self,
            Self::MonoSprite
                | Self::ColorSprite
                | Self::SubpixelSprite
                | Self::MarksGlyph
                | Self::MarksGlyphCoverage
        )
    }

    /// Whether it draws instances out of one of the frame's instance buffers.
    pub fn is_instanced(self) -> bool {
        matches!(
            self,
            Self::Quad
                | Self::Shadow
                | Self::Decoration
                | Self::MonoSprite
                | Self::ColorSprite
                | Self::SubpixelSprite
        )
    }

    /// Whether it reads the block describing the target and the frame's side tables.
    pub fn uses_tables(self) -> bool {
        self.module().uses_tables()
    }

    /// Whether it reads one texture through a block of its own.
    pub fn samples_through_block(self) -> bool {
        matches!(
            self,
            Self::BlurDownsample | Self::BlurAxis | Self::Composite | Self::External
        )
    }

    /// Whether it draws from the frame's array of vector-composite instances.
    ///
    /// It is its own arrangement rather than one of the two above, because it is the only draw that
    /// is both instanced and reads a texture that is not an atlas: one draw call composites a whole
    /// pass one item at a time, each item with its own quad and its own clip.
    pub fn composites_vector(self) -> bool {
        self == Self::VectorComposite
    }

    /// Whether it draws a mark: an item out of the marks lane, and a block and a payload or the
    /// bin pages of its own.
    pub fn draws_marks(self) -> bool {
        matches!(
            self,
            Self::MarksDisc
                | Self::MarksDiscCoverage
                | Self::MarksBox
                | Self::MarksBoxCoverage
                | Self::MarksPolyline
                | Self::MarksPolylineCoverage
                | Self::MarksGlyph
                | Self::MarksGlyphCoverage
                | Self::MarksComposite
        )
    }

    /// Whether it writes coverage into a bin page rather than painting into a target.
    pub fn adds_coverage(self) -> bool {
        matches!(
            self,
            Self::MarksDiscCoverage
                | Self::MarksBoxCoverage
                | Self::MarksPolylineCoverage
                | Self::MarksGlyphCoverage
        )
    }

    /// Whether this pipeline may be built for an attachment of `format`.
    ///
    /// The per-channel coverage pipeline may not be built for an isolated target, and the reason
    /// is not a device limitation: it writes no alpha, because dual-source blending consumes the
    /// per-channel coverage as its blend factor, and that is meaningless against a destination
    /// that is not opaque. An isolated target never is. Text landing in one is emitted as
    /// single-channel coverage instead, so the variant would be unreachable as well as wrong.
    ///
    /// A coverage draw writes only the bin pages. A surface can have their format, so every other
    /// draw may be built for it.
    pub fn suits(self, format: wgpu::TextureFormat) -> bool {
        if self.adds_coverage() && format != Self::COVERAGE_FORMAT {
            return false;
        }
        self != Self::SubpixelSprite || format != crate::target::group_pool::GroupPool::FORMAT
    }

    /// Whether it needs a device feature not every device has.
    ///
    /// Only the per-channel coverage pipeline does. Where the feature is missing the pipeline is
    /// never created and text is emitted as single-channel coverage instead — a fallback rather
    /// than a device that draws no text.
    pub fn needs_dual_source_blending(self) -> bool {
        self == Self::SubpixelSprite
    }

    /// How this pipeline blends with what is already in the attachment.
    ///
    /// Everything composites premultiplied, so the ordinary state is one that expects the source
    /// to be. The per-channel coverage pipeline is the exception in two ways: its blend factor is
    /// the second colour output rather than the source alpha, and it writes no alpha at all —
    /// which is exactly why it is meaningless against a destination that is not opaque.
    pub fn blend(self) -> Option<wgpu::BlendState> {
        match self {
            // A copy, a clear and a filtering pass all replace what was there: a blend would mean
            // the result depended on whatever the attachment happened to hold.
            Self::Blit
            | Self::BlitUndoSrgb
            | Self::DamageClear
            | Self::BlurDownsample
            | Self::BlurAxis => None,
            // Fill coverage adds up, and the format saturates the sum at one: the union of the
            // prims, exact where two fills abut.
            Self::MarksDiscCoverage | Self::MarksBoxCoverage | Self::MarksGlyphCoverage => {
                Some(Self::coverage_blend(wgpu::BlendOperation::Add))
            }
            // The segments of a stroke overlap along its whole length, and many of them can cross
            // one antialiased edge. Their sum makes the stroke too dark there. The largest
            // coverage of each quarter of a pixel is close to the stroke's own.
            Self::MarksPolylineCoverage => Some(Self::coverage_blend(wgpu::BlendOperation::Max)),
            Self::SubpixelSprite => Some(wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::Src1,
                    dst_factor: wgpu::BlendFactor::OneMinusSrc1,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                    operation: wgpu::BlendOperation::Add,
                },
            }),
            _ => Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
        }
    }

    /// The blend of a coverage draw: the source and the bin, combined by `operation`.
    fn coverage_blend(operation: wgpu::BlendOperation) -> wgpu::BlendState {
        let component = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::One,
            operation,
        };
        wgpu::BlendState {
            color: component,
            alpha: component,
        }
    }

    /// Which channels it writes.
    pub fn write_mask(self) -> wgpu::ColorWrites {
        if self == Self::SubpixelSprite {
            wgpu::ColorWrites::COLOR
        } else {
            wgpu::ColorWrites::ALL
        }
    }

    /// A label, so a driver error names the pipeline that produced it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Quad => "zgui.pipeline.quad",
            Self::Shadow => "zgui.pipeline.shadow",
            Self::Decoration => "zgui.pipeline.decoration",
            Self::MonoSprite => "zgui.pipeline.mono_sprite",
            Self::ColorSprite => "zgui.pipeline.color_sprite",
            Self::SubpixelSprite => "zgui.pipeline.subpixel_sprite",
            Self::Blit => "zgui.pipeline.blit",
            Self::BlitUndoSrgb => "zgui.pipeline.blit_undo_srgb",
            Self::DamageClear => "zgui.pipeline.damage_clear",
            Self::BlurDownsample => "zgui.pipeline.blur_downsample",
            Self::BlurAxis => "zgui.pipeline.blur_axis",
            Self::Composite => "zgui.pipeline.composite",
            Self::External => "zgui.pipeline.external",
            Self::VectorComposite => "zgui.pipeline.vector_composite",
            Self::MarksDisc => "zgui.pipeline.marks_disc",
            Self::MarksDiscCoverage => "zgui.pipeline.marks_disc_coverage",
            Self::MarksBox => "zgui.pipeline.marks_box",
            Self::MarksBoxCoverage => "zgui.pipeline.marks_box_coverage",
            Self::MarksPolyline => "zgui.pipeline.marks_polyline",
            Self::MarksPolylineCoverage => "zgui.pipeline.marks_polyline_coverage",
            Self::MarksGlyph => "zgui.pipeline.marks_glyph",
            Self::MarksGlyphCoverage => "zgui.pipeline.marks_glyph_coverage",
            Self::MarksComposite => "zgui.pipeline.marks_composite",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PipelineKind;

    #[test]
    fn only_the_per_channel_pipeline_needs_a_feature_or_withholds_alpha() {
        let gated: Vec<PipelineKind> = PipelineKind::ALL
            .into_iter()
            .filter(|kind| kind.needs_dual_source_blending())
            .collect();
        assert_eq!(gated, vec![PipelineKind::SubpixelSprite]);
        assert_eq!(
            PipelineKind::SubpixelSprite.write_mask(),
            wgpu::ColorWrites::COLOR
        );
        for kind in PipelineKind::ALL {
            if kind == PipelineKind::SubpixelSprite {
                continue;
            }
            assert_eq!(kind.write_mask(), wgpu::ColorWrites::ALL, "{kind:?}");
        }
    }

    #[test]
    fn everything_that_composites_blends_and_everything_that_replaces_does_not() {
        let replacing = [
            PipelineKind::Blit,
            PipelineKind::BlitUndoSrgb,
            PipelineKind::DamageClear,
            PipelineKind::BlurDownsample,
            PipelineKind::BlurAxis,
        ];
        for kind in PipelineKind::ALL {
            assert_eq!(
                kind.blend().is_none(),
                replacing.contains(&kind),
                "{kind:?} blends inconsistently with what it is"
            );
        }
    }

    #[test]
    fn the_per_channel_pipeline_is_the_one_kind_an_isolated_target_may_not_have() {
        use crate::target::group_pool::GroupPool;

        let refused: Vec<PipelineKind> = PipelineKind::ALL
            .into_iter()
            .filter(|kind| !kind.suits(GroupPool::FORMAT) && !kind.adds_coverage())
            .collect();
        assert_eq!(refused, vec![PipelineKind::SubpixelSprite]);
        for kind in PipelineKind::ALL {
            assert_eq!(
                kind.suits(wgpu::TextureFormat::Bgra8Unorm),
                !kind.adds_coverage(),
                "{kind:?} is refused for the composed target"
            );
        }
    }

    #[test]
    fn a_coverage_draw_writes_only_the_bin_pages() {
        for kind in PipelineKind::ALL {
            if !kind.adds_coverage() {
                continue;
            }
            assert!(kind.suits(PipelineKind::COVERAGE_FORMAT), "{kind:?}");
            assert!(kind.draws_marks(), "{kind:?}");
            let blend = kind.blend().expect("coverage blends");
            assert_eq!(blend.color.dst_factor, wgpu::BlendFactor::One, "{kind:?}");
            // The segments of a stroke keep their largest coverage, and fills add theirs.
            let operation = if kind == PipelineKind::MarksPolylineCoverage {
                wgpu::BlendOperation::Max
            } else {
                wgpu::BlendOperation::Add
            };
            assert_eq!(blend.color.operation, operation, "{kind:?}");
        }
    }

    #[test]
    fn a_surface_in_the_bin_format_takes_every_paint_draw() {
        for kind in PipelineKind::ALL {
            assert!(kind.suits(PipelineKind::COVERAGE_FORMAT), "{kind:?}");
        }
    }

    #[test]
    fn a_pipeline_binds_the_tables_exactly_when_its_module_declares_them() {
        for kind in PipelineKind::ALL {
            assert_eq!(
                kind.uses_tables(),
                kind.is_instanced()
                    || kind.draws_marks()
                    || kind.composites_vector()
                    || kind.samples_through_block()
                        && kind != PipelineKind::BlurDownsample
                        && kind != PipelineKind::BlurAxis,
                "{kind:?}"
            );
        }
    }

    #[test]
    fn the_two_copies_share_a_module_and_differ_only_in_the_fragment_they_end_in() {
        assert_eq!(
            PipelineKind::Blit.module(),
            PipelineKind::BlitUndoSrgb.module()
        );
        assert_eq!(
            PipelineKind::Blit.vertex_entry(),
            PipelineKind::BlitUndoSrgb.vertex_entry()
        );
        assert_ne!(
            PipelineKind::Blit.fragment_entry(),
            PipelineKind::BlitUndoSrgb.fragment_entry()
        );
    }

    #[test]
    fn a_glyph_draw_reads_the_atlas_beside_its_payload() {
        use crate::pipeline::marks::MarkKind;

        for coverage in [false, true] {
            let kind = PipelineKind::marks(MarkKind::Glyph, coverage);
            assert!(kind.draws_marks() && kind.samples_atlas(), "{kind:?}");
            assert_eq!(kind.adds_coverage(), coverage, "{kind:?}");
            assert!(!kind.is_instanced(), "{kind:?} reads the marks lane");
        }
        assert_eq!(MarkKind::Glyph.lane(), 3);
    }

    #[test]
    fn every_kind_has_its_own_label_and_entry_points() {
        let mut labels: Vec<&str> = PipelineKind::ALL.iter().map(|k| k.label()).collect();
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), PipelineKind::ALL.len());
    }
}
