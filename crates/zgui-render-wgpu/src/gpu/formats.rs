//! Which formats everything is drawn in, and what to do when the surface offers only an encoded
//! one.
//!
//! One rule decides all of this: **compositing, blending and filtering happen on premultiplied,
//! gamma-encoded values, in every target.** An `*Srgb` attachment format is not a tag — it is a
//! fixed-function decode before every blend and an encode after it — so an `*Srgb` render target
//! silently moves every blend into linear light. The difference is not subtle and it is not
//! theoretical: `rgba(128, 128, 128, 0.5)` over white reads back 191 from a plain attachment,
//! which is what CSS specifies, and 225 from an `*Srgb` one.
//!
//! Nothing above the renderer can see that error, and no image comparison can either: if the
//! target and the surface are both encoded, the final copy round-trips to identity and only the
//! blends in between are wrong. So the choice is pinned here, asserted at configure time, and
//! written to the log at startup.

use zgui_profile::Phase;

/// How an `*Srgb`-only surface is prevented from encoding what is drawn into it.
///
/// Most adapters offer a plain format and none of this applies. The two fallbacks exist because
/// some do not, and because taking the surface's first offered format — which is what happens when
/// nothing decides — is exactly the silent error above.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SrgbTier {
    /// The surface itself is unencoded. Nothing to undo.
    Native,
    /// The surface is encoded, but a view of its unencoded twin can be rendered through.
    ///
    /// The twin is listed in the configuration's view formats and the final copy targets a view of
    /// it, so the encode never happens.
    ViewFormatTwin,
    /// The surface is encoded and cannot be viewed otherwise, so the final copy cancels the encode.
    ///
    /// This is the one shader in the renderer that converts between encodings, and it is legal
    /// precisely because that copy is a pure copy: no blend, no filtering, nothing composited in
    /// the wrong space. It exists because the software and embedded GL path never offers a
    /// mutable-format view.
    UndoInBlit,
}

/// Every format one device draws in, and how the surface's encoding is handled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Formats {
    /// What the surface is configured as.
    pub surface: wgpu::TextureFormat,
    /// What the persistent target a frame composes into is.
    ///
    /// The surface's format with any `*Srgb` suffix removed. Under [`SrgbTier::UndoInBlit`] the
    /// surface itself is encoded and this is where the removal earns its place: taking the
    /// surface's format verbatim would move the encode one step earlier, into every blend.
    pub scene: wgpu::TextureFormat,
    /// What a path rasteriser's scratch is.
    ///
    /// Fixed, because the rasteriser's own contract fixes it. It is reported here so that one
    /// startup line names every format a frame passes through.
    pub scratch: wgpu::TextureFormat,
    /// The view format the final copy renders through, when one is needed.
    pub view_format_twin: Option<wgpu::TextureFormat>,
    /// How the surface's encoding is handled.
    pub tier: SrgbTier,
    /// How the compositor is told to treat the surface's alpha.
    pub alpha_mode: wgpu::CompositeAlphaMode,
}

impl Formats {
    /// The format the final copy's attachment actually has.
    ///
    /// It is the surface's format, unless the surface is being viewed under its unencoded twin —
    /// in which case the attachment *is* that twin, and a pipeline built for the surface's own
    /// format would be rejected against it.
    pub fn present_attachment(&self) -> wgpu::TextureFormat {
        self.view_format_twin.unwrap_or(self.surface)
    }

    /// Whether the final copy has to cancel an encode the attachment will apply.
    pub fn blit_undoes_srgb(&self) -> bool {
        self.tier == SrgbTier::UndoInBlit
    }

    /// The invariant every configuration must satisfy.
    ///
    /// An encoded surface is only acceptable when something cancels the encode. Without this, an
    /// adapter that offers no plain format renders every blend in linear light and every pixel
    /// test still passes, because the tests compose into a target they chose themselves.
    pub fn is_sound(&self) -> bool {
        !self.surface.is_srgb() || self.blit_undoes_srgb() || self.view_format_twin.is_some()
    }

    /// The view formats the surface has to be configured with.
    pub fn view_formats(&self) -> Vec<wgpu::TextureFormat> {
        self.view_format_twin.into_iter().collect()
    }

    /// Writes the one line that makes a whole class of colour bug diagnosable.
    ///
    /// Four formats and a tier, at startup, is the entire diagnostic for "the blending looks
    /// slightly wrong on this machine" — a symptom that no golden, on any machine, can reproduce.
    pub fn log(&self, adapter: &str) {
        let _startup = Phase::Render.span().entered();
        tracing::info!(
            adapter,
            surface = ?self.surface,
            scene = ?self.scene,
            scratch = ?self.scratch,
            alpha_mode = ?self.alpha_mode,
            tier = ?self.tier,
            "renderer formats"
        );
    }
}

/// Picks the formats and the alpha mode for a surface with these capabilities.
///
/// `mutable_texture_formats` is the device's own answer, not the adapter's advertisement, because
/// it decides between the two fallbacks and getting it wrong is invisible.
pub fn choose(
    formats: &[wgpu::TextureFormat],
    alpha_modes: &[wgpu::CompositeAlphaMode],
    opaque: bool,
    mutable_texture_formats: bool,
) -> Formats {
    let (surface, tier) = choose_surface_format(formats, mutable_texture_formats);
    let twin = match tier {
        SrgbTier::ViewFormatTwin => Some(surface.remove_srgb_suffix()),
        SrgbTier::Native | SrgbTier::UndoInBlit => None,
    };
    Formats {
        surface,
        scene: surface.remove_srgb_suffix(),
        scratch: wgpu::TextureFormat::Rgba8Unorm,
        view_format_twin: twin,
        tier,
        alpha_mode: choose_alpha_mode(alpha_modes, opaque),
    }
}

/// The surface format, in preference order, and what has to be done about its encoding.
///
/// `Bgra8Unorm` first because it is what Linux compositors hand out and it avoids a swizzle;
/// `Rgba8Unorm` next; then any other unencoded format the surface offers. Only when every offered
/// format is encoded does a fallback tier apply.
fn choose_surface_format(
    formats: &[wgpu::TextureFormat],
    mutable_texture_formats: bool,
) -> (wgpu::TextureFormat, SrgbTier) {
    let preferred = [
        wgpu::TextureFormat::Bgra8Unorm,
        wgpu::TextureFormat::Rgba8Unorm,
    ];
    for candidate in preferred {
        if formats.contains(&candidate) {
            return (candidate, SrgbTier::Native);
        }
    }
    if let Some(plain) = formats.iter().copied().find(|format| !format.is_srgb()) {
        return (plain, SrgbTier::Native);
    }
    // Every offered format encodes. Prefer the twin of the format we would have wanted, so the
    // channel order still matches what the compositor prefers.
    let encoded = preferred
        .iter()
        .map(|format| format.add_srgb_suffix())
        .find(|format| formats.contains(format))
        .or_else(|| formats.first().copied())
        // A surface with no formats at all is a surface incompatible with the adapter, which the
        // candidate loop rejects before this is reached. Naming a format here keeps this function
        // total rather than leaving a panic where a decision belongs.
        .unwrap_or(wgpu::TextureFormat::Bgra8UnormSrgb);
    let tier = if mutable_texture_formats {
        SrgbTier::ViewFormatTwin
    } else {
        SrgbTier::UndoInBlit
    };
    (encoded, tier)
}

/// How the compositor should treat the surface's alpha.
///
/// Pinned rather than left to `Auto`, which is a deferral dressed as a default. `PostMultiplied` is
/// excluded on principle: it would require an un-premultiply step that exists nowhere in this
/// pipeline, so offering it would mean two answers to what a colour is.
fn choose_alpha_mode(
    alpha_modes: &[wgpu::CompositeAlphaMode],
    opaque: bool,
) -> wgpu::CompositeAlphaMode {
    if opaque {
        return pick(alpha_modes, wgpu::CompositeAlphaMode::Opaque);
    }
    if alpha_modes.contains(&wgpu::CompositeAlphaMode::PreMultiplied) {
        return wgpu::CompositeAlphaMode::PreMultiplied;
    }
    tracing::info!(
        "the surface offers no premultiplied alpha mode; the window is composited opaque instead"
    );
    pick(alpha_modes, wgpu::CompositeAlphaMode::Opaque)
}

/// `wanted` when it is offered, and the first offered mode otherwise.
fn pick(
    alpha_modes: &[wgpu::CompositeAlphaMode],
    wanted: wgpu::CompositeAlphaMode,
) -> wgpu::CompositeAlphaMode {
    if alpha_modes.contains(&wanted) {
        wanted
    } else {
        alpha_modes
            .first()
            .copied()
            .unwrap_or(wgpu::CompositeAlphaMode::Opaque)
    }
}

#[cfg(test)]
mod tests {
    use super::{Formats, SrgbTier, choose};
    use wgpu::CompositeAlphaMode as Alpha;
    use wgpu::TextureFormat as Format;

    /// The alpha modes every surface offers at least one of.
    const OPAQUE_ONLY: [Alpha; 1] = [Alpha::Opaque];

    #[test]
    fn a_plain_format_is_preferred_over_the_encoded_one_the_driver_lists_first() {
        // Raw driver order on ordinary Linux hardware puts the encoded format first, and that is
        // precisely what wgpu's own default configuration would take.
        let offered = [Format::Bgra8UnormSrgb, Format::Bgra8Unorm];
        let formats = choose(&offered, &OPAQUE_ONLY, true, true);
        assert_eq!(formats.surface, Format::Bgra8Unorm);
        assert_eq!(formats.scene, Format::Bgra8Unorm);
        assert_eq!(formats.tier, SrgbTier::Native);
        assert!(formats.is_sound());
    }

    #[test]
    fn rgba_is_taken_when_bgra_is_not_offered() {
        let formats = choose(
            &[Format::Rgba8UnormSrgb, Format::Rgba8Unorm],
            &OPAQUE_ONLY,
            true,
            true,
        );
        assert_eq!(formats.surface, Format::Rgba8Unorm);
    }

    #[test]
    fn any_unencoded_format_beats_every_encoded_one() {
        let formats = choose(
            &[Format::Bgra8UnormSrgb, Format::Rgb10a2Unorm],
            &OPAQUE_ONLY,
            true,
            true,
        );
        assert_eq!(formats.surface, Format::Rgb10a2Unorm);
        assert_eq!(formats.tier, SrgbTier::Native);
    }

    #[test]
    fn an_encoded_only_surface_with_mutable_views_renders_through_the_twin() {
        let formats = choose(&[Format::Bgra8UnormSrgb], &OPAQUE_ONLY, true, true);
        assert_eq!(formats.surface, Format::Bgra8UnormSrgb);
        assert_eq!(formats.tier, SrgbTier::ViewFormatTwin);
        assert_eq!(formats.view_format_twin, Some(Format::Bgra8Unorm));
        assert_eq!(formats.view_formats(), vec![Format::Bgra8Unorm]);
        assert!(!formats.blit_undoes_srgb());
        assert!(formats.is_sound());
    }

    #[test]
    fn an_encoded_only_surface_without_mutable_views_cancels_the_encode_in_the_copy() {
        let formats = choose(&[Format::Rgba8UnormSrgb], &OPAQUE_ONLY, true, false);
        assert_eq!(formats.surface, Format::Rgba8UnormSrgb);
        assert_eq!(formats.tier, SrgbTier::UndoInBlit);
        assert_eq!(formats.view_format_twin, None);
        assert!(formats.blit_undoes_srgb());
        assert!(formats.is_sound());
    }

    #[test]
    fn the_composed_target_is_never_an_encoded_format() {
        for offered in [
            vec![Format::Bgra8Unorm],
            vec![Format::Bgra8UnormSrgb],
            vec![Format::Rgba8UnormSrgb],
        ] {
            for mutable in [true, false] {
                let formats = choose(&offered, &OPAQUE_ONLY, true, mutable);
                assert!(
                    !formats.scene.is_srgb(),
                    "{offered:?} with mutable={mutable} composed into {:?}",
                    formats.scene
                );
                assert!(formats.is_sound());
            }
        }
    }

    #[test]
    fn an_unsound_configuration_is_recognised_as_one() {
        // Not reachable through `choose`, which is the point: the assertion has to be able to fail
        // for it to be worth making at configure time.
        let broken = Formats {
            surface: Format::Bgra8UnormSrgb,
            scene: Format::Bgra8UnormSrgb,
            scratch: Format::Rgba8Unorm,
            view_format_twin: None,
            tier: SrgbTier::Native,
            alpha_mode: Alpha::Opaque,
        };
        assert!(!broken.is_sound());
    }

    #[test]
    fn a_translucent_window_asks_for_premultiplied_alpha_and_settles_for_opaque() {
        let both = [Alpha::Opaque, Alpha::PreMultiplied];
        assert_eq!(
            choose(&[Format::Bgra8Unorm], &both, false, true).alpha_mode,
            Alpha::PreMultiplied
        );
        assert_eq!(
            choose(&[Format::Bgra8Unorm], &OPAQUE_ONLY, false, true).alpha_mode,
            Alpha::Opaque
        );
    }

    #[test]
    fn an_opaque_window_is_never_composited_as_translucent_and_never_left_to_auto() {
        let all = [
            Alpha::Auto,
            Alpha::PreMultiplied,
            Alpha::PostMultiplied,
            Alpha::Opaque,
        ];
        assert_eq!(
            choose(&[Format::Bgra8Unorm], &all, true, true).alpha_mode,
            Alpha::Opaque
        );
    }

    #[test]
    fn post_multiplied_is_never_chosen_even_when_it_is_the_only_translucent_mode() {
        let offered = [Alpha::Opaque, Alpha::PostMultiplied];
        assert_eq!(
            choose(&[Format::Bgra8Unorm], &offered, false, true).alpha_mode,
            Alpha::Opaque
        );
    }
}
