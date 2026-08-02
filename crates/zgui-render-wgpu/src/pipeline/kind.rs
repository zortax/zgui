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
}

impl PipelineKind {
    /// Every kind.
    pub const ALL: [Self; 14] = [
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
    ];

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
        }
    }

    /// Whether it reads an atlas texture.
    pub fn samples_atlas(self) -> bool {
        matches!(
            self,
            Self::MonoSprite | Self::ColorSprite | Self::SubpixelSprite
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

    /// Whether this pipeline may be built for an attachment of `format`.
    ///
    /// The per-channel coverage pipeline may not be built for an isolated target, and the reason
    /// is not a device limitation: it writes no alpha, because dual-source blending consumes the
    /// per-channel coverage as its blend factor, and that is meaningless against a destination
    /// that is not opaque. An isolated target never is. Text landing in one is emitted as
    /// single-channel coverage instead, so the variant would be unreachable as well as wrong.
    pub fn suits(self, format: wgpu::TextureFormat) -> bool {
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
            if kind != PipelineKind::SubpixelSprite {
                assert_eq!(kind.write_mask(), wgpu::ColorWrites::ALL, "{kind:?}");
            }
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
            .filter(|kind| !kind.suits(GroupPool::FORMAT))
            .collect();
        assert_eq!(refused, vec![PipelineKind::SubpixelSprite]);
        for kind in PipelineKind::ALL {
            assert!(
                kind.suits(wgpu::TextureFormat::Bgra8Unorm),
                "{kind:?} is refused for the composed target"
            );
        }
    }

    #[test]
    fn a_pipeline_binds_the_tables_exactly_when_its_module_declares_them() {
        for kind in PipelineKind::ALL {
            assert_eq!(
                kind.uses_tables(),
                kind.is_instanced()
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
    fn every_kind_has_its_own_label_and_entry_points() {
        let mut labels: Vec<&str> = PipelineKind::ALL.iter().map(|k| k.label()).collect();
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), PipelineKind::ALL.len());
    }
}
