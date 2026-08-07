//! The renderer contract.

use zgui_atlas::TextureSink;
use zgui_bits::DamageSet;
use zgui_scene::Scene;

use crate::capabilities::RenderCapabilities;
use crate::memory::MemoryReport;
use crate::outcome::FrameOutcome;
use crate::pool::TargetPoolReport;
use crate::shift::ScrollShift;
use crate::target::RenderTarget;
use crate::texture::{ExternalTexture, TextureHandle};

/// Something that can put a display list on a screen.
///
/// The whole surface of a renderer is here, and it names no graphics API, so a second one — a
/// capture implementation for tests, a software one, one over a different graphics API — is an
/// ordinary implementation rather than a fork.
///
/// # Damage
///
/// [`Renderer::draw`] is given the rectangles that must be redrawn, and everything outside them is
/// expected to still hold the previous frame's pixels. That is only legal because a renderer
/// composes into a target it keeps, rather than straight into whatever it is about to present; an
/// implementation that composes into a transient surface has to treat every frame as full.
///
/// **Damage is retired when the frame's work was submitted, not when a frame was presented** — see
/// [`FrameOutcome::retires_damage`], which is the authority on which outcomes those are.
pub trait Renderer {
    /// What the device underneath can do.
    ///
    /// Read before a frame is built, not after: whether text can be antialiased per colour channel
    /// changes which primitives the display list should contain, not only how they are drawn.
    fn capabilities(&self) -> RenderCapabilities;

    /// Points the renderer at a surface, or at a resized one.
    ///
    /// Everything sized against the surface is reallocated, and the next frame redraws all of it,
    /// because nothing observed what happened to the surface in between.
    fn configure(&mut self, target: RenderTarget);

    /// The surface currently being drawn for, or `None` before one has been configured.
    fn target(&self) -> Option<RenderTarget>;

    /// Draws `scene`, redrawing only what `damage` covers.
    ///
    /// `scene` must be finished: its arrays have to be in draw order and its vector passes planned,
    /// because a renderer executes that plan rather than deriving one.
    fn draw(&mut self, scene: &Scene, damage: &DamageSet) -> FrameOutcome;

    /// Whether this renderer can move pixels it has already composed.
    ///
    /// A renderer answering true keeps a composed target across frames, so a scroll can be answered
    /// by translating the part of it that is still valid and drawing only the strip that is not.
    /// One answering false is drawn for in full, exactly as before — which is why this is a
    /// question and not an instruction: the caller narrows the damage **only** when the renderer
    /// says it will make up the difference, and a renderer that cannot is never handed a frame
    /// whose damage is short of what it has to draw.
    ///
    /// False by default, so a renderer that has not thought about it is correct.
    fn shifts_composed_pixels(&self) -> bool {
        false
    }

    /// Moves the still-valid pixels of a composed region, before the next [`Renderer::draw`].
    ///
    /// Only ever called on a renderer that answered true to
    /// [`shifts_composed_pixels`](Renderer::shifts_composed_pixels), and always in the same frame as
    /// the draw whose damage was narrowed for it. The caller has already established that the
    /// region moved rigidly by whole pixels and that nothing else is drawn over it; what is owed
    /// here is the copy and nothing else.
    fn shift_composed(&mut self, shift: ScrollShift) {
        let _ = shift;
    }

    /// Registers a texture the renderer did not draw, so the display list can refer to it.
    ///
    /// Returns the renderer's own handle for it.
    fn register_external(&mut self, texture: ExternalTexture) -> TextureHandle;

    /// Forgets a registered external texture.
    fn release_external(&mut self, handle: TextureHandle);

    /// What the renderer is currently holding, including anything its rasteriser holds.
    fn memory(&self) -> MemoryReport;

    /// The reusable targets isolated content is composed in.
    ///
    /// Defaults to [`TargetPoolReport::EMPTY`], because most implementations of this trait compose
    /// straight into what they were given and pool nothing at all — for those, empty is the true
    /// answer rather than a stub. An implementation that *does* pool and leaves this defaulted is
    /// caught rather than believed: a window's budget asserts that every cache it registers is empty
    /// after being told to forget, and a pool that reports nothing and frees nothing still shows up
    /// in [`Renderer::memory`].
    fn target_pool(&self) -> TargetPoolReport {
        TargetPoolReport::EMPTY
    }

    /// Frees every pooled target that is not lent out, and reports how many bytes that returned.
    ///
    /// Costs the next frame that isolates anything a reallocation and nothing else: a pooled target
    /// holds one frame's isolated content and is cleared before it is drawn into, so nothing is lost
    /// with it.
    fn release_cached_targets(&mut self) -> u64 {
        0
    }

    /// Frees cold high-water resources after a wall-clock idle grace period.
    ///
    /// Unlike an explicit cache forget, this keeps live presentation state and an initialized
    /// vector rasteriser. The default can only name the portable target-pool seam.
    fn release_idle_resources(&mut self) -> u64 {
        self.release_cached_targets()
    }

    /// How long the last frame waited to be handed a surface to present into.
    ///
    /// A queued presentation mode makes the acquisition the loop's only brake: with every image in
    /// the swap chain already spoken for, the call that asks for the next one blocks until the
    /// display releases one. How long it blocked is therefore how much slack the frame had — how
    /// much earlier than necessary it was started — and it is the one observation from which a
    /// caller can recover the display's cadence without asking the window system for it.
    ///
    /// Defaults to zero, which is the true answer for an implementation that presents to nothing
    /// and for one whose surface always has an image spare: neither made the frame wait.
    fn acquire_block(&self) -> core::time::Duration {
        core::time::Duration::ZERO
    }

    /// Where rasterised content is uploaded: the device side of the texture atlas.
    ///
    /// The atlas's *policy* — which tile goes where, what is held, what is evicted — belongs above
    /// a renderer and is decided with no device in sight. What a renderer owns is the textures
    /// themselves, because they have to be created on the device that will sample them, and this
    /// is the seam between the two.
    ///
    /// Handed out mutably and per call rather than held by whoever caches: the textures do not
    /// survive [`configure`](Renderer::configure) on a lost device, so a borrow kept across frames
    /// would outlive what it names.
    fn texture_sink(&mut self) -> &mut dyn TextureSink;

    /// This renderer as its concrete type, for a backend-specific companion crate.
    ///
    /// The contract above deliberately names no graphics API, and almost everything lives happily
    /// behind it. The one thing that cannot is *handing over a texture*: an embedded producer's
    /// texture is a device resource, and attaching it is an operation only the backend that owns
    /// the device can define. This is the door that companion walks through, and `None` — the
    /// default — is a renderer saying it has no such door: a capture renderer answers `None` and
    /// an embed host degrades to bookkeeping, which is exactly what a headless test wants.
    ///
    /// **A wrapper renderer must forward this to what it wraps**, or everything behind the wrapper
    /// silently loses the capability.
    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        None
    }
}
