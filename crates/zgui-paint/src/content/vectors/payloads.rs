//! Mark payloads kept between frames by the identity of what they were built from.
//!
//! A mark payload uploads once per allocation: the renderer keeps it resident by its address. A
//! shape recognised from a path the recognition cache holds is lowered to the same payload
//! allocation on every encode while its source lives, so a pan or a zoom of a canvas view uploads
//! no payload.

use std::sync::{Arc, Weak};

use rustc_hash::FxHashMap;
use zgui_scene::MarkPayload;

use crate::emit::vector::recognise::Decomposition;

/// How many frames a shape entry survives without a lookup.
const SHAPE_FRAMES: u32 = 2;

/// The most shape entries held. A full map keeps what it holds and adds nothing.
const MAX_SHAPES: usize = 4096;

/// One shape's payload.
#[derive(Debug)]
struct ShapeEntry {
    /// The recognition it was lowered from, held so no other takes its address.
    found: Weak<Decomposition>,
    /// The payload.
    payload: Arc<MarkPayload>,
    /// The [`zgui_scene::MarkFlags`] the lowering found.
    flags: u32,
    /// The frame it was last looked up in.
    touched: u32,
}

/// Mark payloads by the identity of their source.
#[doc(hidden)]
#[derive(Debug, Default)]
pub struct MarkPayloads {
    /// Shape payloads, by the address of the recognition and whether the item is a union.
    shapes: FxHashMap<(usize, bool), ShapeEntry>,
    /// The current frame.
    frame: u32,
}

impl MarkPayloads {
    /// Starts a frame.
    pub(crate) fn begin_frame(&mut self) {
        self.frame = self.frame.wrapping_add(1);
    }

    /// Ends a frame: drops the entries no frame asked for lately.
    pub(crate) fn end_frame(&mut self) {
        let frame = self.frame;
        self.shapes
            .retain(|_, entry| frame.wrapping_sub(entry.touched) < SHAPE_FRAMES);
    }

    /// Forgets everything.
    pub(crate) fn clear(&mut self) {
        self.shapes.clear();
    }

    /// The payload and flags lowered from `found` before, if `found` is still that recognition.
    pub(crate) fn shape(
        &mut self,
        found: &Arc<Decomposition>,
        union: bool,
    ) -> Option<(Arc<MarkPayload>, u32)> {
        let frame = self.frame;
        let entry = self.shapes.get_mut(&(Arc::as_ptr(found) as usize, union))?;
        if !Weak::ptr_eq(&entry.found, &Arc::downgrade(found)) || entry.found.strong_count() == 0 {
            return None;
        }
        entry.touched = frame;
        Some((Arc::clone(&entry.payload), entry.flags))
    }

    /// Keeps the payload and flags lowered from `found`. A full map adds nothing.
    pub(crate) fn insert_shape(
        &mut self,
        found: &Arc<Decomposition>,
        union: bool,
        payload: Arc<MarkPayload>,
        flags: u32,
    ) {
        if self.shapes.len() >= MAX_SHAPES {
            return;
        }
        self.shapes.insert(
            (Arc::as_ptr(found) as usize, union),
            ShapeEntry {
                found: Arc::downgrade(found),
                payload,
                flags,
                touched: self.frame,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use zgui_scene::MarkPayload;

    use super::MarkPayloads;
    use crate::emit::vector::recognise::{Decomposition, Orientation};

    /// One disc.
    fn found() -> Arc<Decomposition> {
        Arc::new(Decomposition {
            discs: vec![[4.0, 4.0, 2.0, 0.0]],
            boxes: Vec::new(),
            capsules: Vec::new(),
            caps: Vec::new(),
            half_width: 0.0,
            ink: [2.0, 2.0, 6.0, 6.0],
            orientation: Orientation::Positive,
            count: 1,
            max_extent: 4.0,
        })
    }

    #[test]
    fn a_shape_payload_is_reused_by_its_recognition() {
        let mut payloads = MarkPayloads::default();
        payloads.begin_frame();
        let found = found();
        let payload = Arc::new(MarkPayload::default());
        payloads.insert_shape(&found, true, Arc::clone(&payload), 7);
        let (held, flags) = payloads.shape(&found, true).expect("held");
        assert!(Arc::ptr_eq(&held, &payload));
        assert_eq!(flags, 7);
        assert!(
            payloads.shape(&found, false).is_none(),
            "the union is part of the key"
        );
        assert!(
            payloads.shape(&self::found(), true).is_none(),
            "another recognition misses"
        );

        // Touched every frame, the entry stays; untouched for two frames, it goes.
        for _ in 0..4 {
            payloads.end_frame();
            payloads.begin_frame();
            assert!(payloads.shape(&found, true).is_some());
        }
        for _ in 0..3 {
            payloads.end_frame();
            payloads.begin_frame();
        }
        assert!(payloads.shape(&found, true).is_none());
    }
}
