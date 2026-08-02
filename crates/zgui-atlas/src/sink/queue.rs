//! Texture lifetimes, recorded now and performed later.
//!
//! Writing texels was always deferred — the bytes are queued and leave in one batch — but creating
//! and destroying a texture was not, so allocating room for a glyph could reach a device. That is
//! the one thing that made rasterising into an atlas need something to rasterise *into*, and the
//! reason a walk that only reads a document had to borrow a renderer to run.
//!
//! Recording the two calls makes the whole of [`Atlas`](crate::Atlas) a data structure: the caller
//! that has a device replays the log into it, at a moment of its own choosing, alongside the
//! uploads that were already waiting there.

use zgui_geom::{Device, Size};

use crate::sink::TextureSink;
use crate::sink::error::SinkError;
use crate::texture::{TextureFormat, TextureId};

/// One thing a device is going to be asked to do to a texture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TextureOp {
    /// Bring `texture` into existence with room for `size` texels of `format`.
    Create {
        /// Which texture.
        texture: TextureId,
        /// How large.
        size: Size<i32, Device>,
        /// What its texels mean.
        format: TextureFormat,
    },
    /// Release `texture`.
    Destroy {
        /// Which texture.
        texture: TextureId,
    },
}

/// The texture creations and destructions an atlas has decided on and not yet performed.
///
/// Kept as one ordered log rather than two lists, because texture slots are reused: destroying slot
/// three and then creating a new texture in it is an ordinary sequence, and replaying the two out of
/// order would destroy the texture that had just been made.
#[derive(Debug, Default)]
pub(crate) struct TextureQueue {
    /// What is waiting, oldest first.
    ops: Vec<TextureOp>,
}

impl TextureQueue {
    /// An empty queue.
    pub(crate) fn new() -> Self {
        Self { ops: Vec::new() }
    }

    /// Records that `texture` is to be created.
    pub(crate) fn create(
        &mut self,
        texture: TextureId,
        size: Size<i32, Device>,
        format: TextureFormat,
    ) {
        self.ops.push(TextureOp::Create {
            texture,
            size,
            format,
        });
    }

    /// Records that `texture` is to be destroyed.
    ///
    /// A texture whose creation is still waiting here is cancelled instead: the device never heard
    /// of it, so asking it to release one it never made is at best ignored and at worst a diagnostic
    /// about an unknown handle. This is the path a rejected allocation takes, where a texture is
    /// made and given straight back within one walk.
    pub(crate) fn destroy(&mut self, texture: TextureId) {
        let unborn = self.ops.iter().rposition(|op| match op {
            TextureOp::Create { texture: made, .. } => *made == texture,
            TextureOp::Destroy { texture: gone } => *gone == texture,
        });
        if let Some(index) = unborn
            && matches!(self.ops[index], TextureOp::Create { .. })
        {
            self.ops.remove(index);
            return;
        }
        self.ops.push(TextureOp::Destroy { texture });
    }

    /// How many calls are waiting.
    pub(crate) fn len(&self) -> usize {
        self.ops.len()
    }

    /// Performs every waiting call against `sink`, oldest first, and empties the queue.
    ///
    /// A refusal stops the replay and leaves that call and everything after it queued, so a caller
    /// that recovers replays again rather than having lost the textures its tiles are inside.
    ///
    /// # Errors
    ///
    /// [`SinkError`] as the sink reported it.
    pub(crate) fn replay(&mut self, sink: &mut impl TextureSink) -> Result<(), SinkError> {
        let mut done = 0;
        let mut failure = None;
        for op in &self.ops {
            match *op {
                TextureOp::Create {
                    texture,
                    size,
                    format,
                } => {
                    if let Err(error) = sink.create_texture(texture, size, format) {
                        failure = Some(error);
                        break;
                    }
                }
                TextureOp::Destroy { texture } => sink.destroy_texture(texture),
            }
            done += 1;
        }
        self.ops.drain(..done);
        match failure {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use zgui_geom::Size;

    use super::TextureQueue;
    use crate::sink::{MemorySink, TextureSink};
    use crate::texture::{TextureId, TextureKind};

    /// A texture of the mono pool.
    fn mono(index: u32) -> TextureId {
        TextureId::new(TextureKind::Mono, index)
    }

    #[test]
    fn a_texture_destroyed_before_it_was_ever_made_is_never_mentioned_to_a_device() {
        let mut queue = TextureQueue::new();
        queue.create(mono(0), Size::new(64, 64), TextureKind::Mono.format());
        queue.destroy(mono(0));
        assert_eq!(queue.len(), 0, "the pair cancels rather than replaying");

        let mut sink = MemorySink::new();
        queue.replay(&mut sink).expect("nothing to replay");
        assert_eq!(sink.textures_created(), 0);
    }

    #[test]
    fn a_slot_destroyed_and_filled_again_replays_in_the_order_it_happened() {
        let mut sink = MemorySink::new();
        sink.create_texture(mono(0), Size::new(64, 64), TextureKind::Mono.format())
            .expect("a fresh sink accepts a texture");

        let mut queue = TextureQueue::new();
        queue.destroy(mono(0));
        queue.create(mono(0), Size::new(128, 128), TextureKind::Mono.format());
        queue.replay(&mut sink).expect("the sink accepts both");

        assert_eq!(
            sink.live_textures(),
            1,
            "the destroy ran first, so the slot holds the texture that was made second"
        );
        assert_eq!(sink.size_of(mono(0)), Some(Size::new(128, 128)));
    }
}
