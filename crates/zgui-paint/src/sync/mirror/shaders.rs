//! Declared application shaders mirrored as compositor shaders.

use compositor_types::ShaderReads;
use compositor_types::{ShaderId, Transaction};
use rustc_hash::{FxHashMap, FxHashSet};

/// The compositor shader of each declared shader an item drew with.
///
/// A shader is created the first time an item or a filter names it, and lives as long as the
/// client.
#[derive(Debug, Default)]
pub(crate) struct Shaders {
    ids: FxHashMap<zgui_css::shader::ShaderId, ShaderId>,
    timed: FxHashSet<ShaderId>,
}

impl Shaders {
    /// Returns the compositor shader of `id`, creating it from its program on first use.
    ///
    /// Returns `None` for a handle no program was declared under.
    pub(crate) fn ensure(
        &mut self,
        tx: &mut Transaction,
        id: zgui_css::shader::ShaderId,
    ) -> Option<ShaderId> {
        if let Some(shader) = self.ids.get(&id) {
            return Some(*shader);
        }
        let desc = crate::content::shader::program(id)?;
        let reads_time = desc.reads.contains(ShaderReads::TIME);
        let shader = tx.create_shader(desc);
        if reads_time {
            self.timed.insert(shader);
        }
        self.ids.insert(id, shader);
        Some(shader)
    }

    /// Reports whether the shader `id` reads the clock.
    pub(crate) fn reads_time(&self, id: ShaderId) -> bool {
        self.timed.contains(&id)
    }
}
