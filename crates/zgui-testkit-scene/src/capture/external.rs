//! The registry of textures the renderer did not draw.
//!
//! A capture renderer has no textures, but it still has to *answer* for them: the display list
//! refers to an external texture by id, and a handle handed out for one that was later released has
//! to stop resolving. Recording that here is what lets a test drive the whole registration
//! lifecycle without a device.

use zgui_render::{ExternalTexture, TextureHandle};

/// Registered external textures, in registration order.
#[derive(Clone, Debug, Default)]
pub struct Externals {
    /// The textures currently registered.
    registered: Vec<ExternalTexture>,
    /// The next handle to hand out.
    next: u64,
}

impl Externals {
    /// An empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers `texture` under a fresh handle and returns it.
    ///
    /// Handles are never reused, so a stale handle stays stale instead of resolving to whatever was
    /// registered next — which is the failure a registry that recycled its slots would produce, and
    /// the one nothing else would notice.
    pub fn register(&mut self, mut texture: ExternalTexture) -> TextureHandle {
        self.next += 1;
        let handle = TextureHandle(self.next);
        texture.handle = handle;
        self.registered.push(texture);
        handle
    }

    /// Forgets `handle`, and reports whether it was registered.
    pub fn release(&mut self, handle: TextureHandle) -> bool {
        let before = self.registered.len();
        self.registered.retain(|texture| texture.handle != handle);
        self.registered.len() != before
    }

    /// The texture `handle` resolves to.
    pub fn get(&self, handle: TextureHandle) -> Option<&ExternalTexture> {
        self.registered
            .iter()
            .find(|texture| texture.handle == handle)
    }

    /// Every registered texture, in registration order.
    pub fn all(&self) -> &[ExternalTexture] {
        &self.registered
    }

    /// How many are registered.
    pub fn len(&self) -> usize {
        self.registered.len()
    }

    /// Whether none are registered.
    pub fn is_empty(&self) -> bool {
        self.registered.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use zgui_geom::Size;
    use zgui_render::ExternalTexture;
    use zgui_scene::ExternalTextureId;

    use super::Externals;

    /// A texture to register.
    fn texture(id: u64) -> ExternalTexture {
        ExternalTexture {
            id: ExternalTextureId(id),
            handle: zgui_render::TextureHandle(0),
            size: Size::new(16, 16),
            premultiplied: true,
        }
    }

    #[test]
    fn a_released_handle_never_resolves_to_a_later_registration() {
        let mut registry = Externals::new();
        let first = registry.register(texture(1));
        assert!(registry.release(first));
        let second = registry.register(texture(2));

        assert_ne!(first, second, "handles are not recycled");
        assert!(registry.get(first).is_none());
        assert_eq!(registry.get(second).map(|texture| texture.id.0), Some(2));
    }

    #[test]
    fn releasing_something_that_was_never_registered_reports_it() {
        let mut registry = Externals::new();
        assert!(!registry.release(zgui_render::TextureHandle(9)));
        assert!(registry.is_empty());
    }
}
