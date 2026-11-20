//! What the vector routes remember between frames: route declines, recognitions, mark payloads
//! and path splits.

use rustc_hash::FxHashMap;
use zgui_scene::VectorId;

/// Where the vector routes keep what they found, and the route history of each shape.
pub trait VectorMaskSource {
    /// Whether `owner` may try the analytic route this frame.
    ///
    /// False for a few frames after a large shape failed recognition, so a path that is no
    /// analytic shape is not measured again on every frame.
    fn analytic(&self, _owner: VectorId) -> bool {
        true
    }

    /// Records that a large shape of `owner` failed recognition.
    fn analytic_declined(&self, _owner: VectorId) {}

    /// Whether `owner` may try the marks route this frame.
    ///
    /// False for a few frames after a large shape failed recognition there.
    fn marks(&self, _owner: VectorId) -> bool {
        true
    }

    /// Records that a large shape of `owner` failed recognition on the marks route.
    fn marks_declined(&self, _owner: VectorId) {}

    /// Where recognitions are kept between frames, if anywhere.
    #[doc(hidden)]
    fn recognitions(&self) -> Option<core::cell::RefMut<'_, super::recognitions::Recognitions>> {
        None
    }

    /// Where mark payloads are kept between frames, if anywhere.
    #[doc(hidden)]
    fn payloads(&self) -> Option<core::cell::RefMut<'_, super::payloads::MarkPayloads>> {
        None
    }

    /// Whether a series item draws its whole payload however little of it is shown. For a
    /// reference picture.
    #[doc(hidden)]
    fn series_whole(&self) -> bool {
        false
    }

    /// Where whole drawings are rasterised on the CPU, if anywhere.
    #[doc(hidden)]
    fn layers(&self) -> Option<&dyn super::layer::VectorLayerSource> {
        None
    }

    /// Whether `owner` may try the path glyph route this frame.
    ///
    /// False for a few frames after a large shape failed to split into repeated outlines.
    fn path_glyphs(&self, _owner: VectorId) -> bool {
        true
    }

    /// Records that a large shape of `owner` failed to split into repeated outlines.
    fn path_glyphs_declined(&self, _owner: VectorId) {}

    /// Where path splits are kept between frames, if anywhere.
    #[doc(hidden)]
    fn glyph_splits(&self) -> Option<core::cell::RefMut<'_, super::path_glyphs::Splits>> {
        None
    }
}

/// A source that declines the analytic route, the marks route and the path glyph route.
///
/// Every shape drawn through it takes the general route.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoVectorMasks;

impl VectorMaskSource for NoVectorMasks {
    fn analytic(&self, _owner: VectorId) -> bool {
        false
    }

    fn marks(&self, _owner: VectorId) -> bool {
        false
    }

    fn path_glyphs(&self, _owner: VectorId) -> bool {
        false
    }
}

/// A source that declines the marks route and the path glyph route, and allows the analytic
/// route.
///
/// It remembers nothing, so a shape that fails recognition is measured again on every frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct AnalyticOnly;

impl VectorMaskSource for AnalyticOnly {
    fn marks(&self, _owner: VectorId) -> bool {
        false
    }

    fn path_glyphs(&self, _owner: VectorId) -> bool {
        false
    }
}

/// A source that declines the analytic route and the path glyph route, and allows the marks
/// route.
///
/// It remembers nothing, so a shape that fails recognition is measured again on every frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct MarksOnly;

impl VectorMaskSource for MarksOnly {
    fn analytic(&self, _owner: VectorId) -> bool {
        false
    }

    fn path_glyphs(&self, _owner: VectorId) -> bool {
        false
    }
}

/// A source that declines the analytic route and the path glyph route, allows the marks route,
/// and keeps recognitions and mark payloads between frames. For tests.
#[doc(hidden)]
#[derive(Debug, Default)]
pub struct CachedMarks {
    /// The recognitions.
    pub(crate) recognitions: core::cell::RefCell<super::recognitions::Recognitions>,
    /// The mark payloads.
    pub(crate) payloads: core::cell::RefCell<super::payloads::MarkPayloads>,
    /// Whether series items draw their whole payloads.
    pub(crate) whole: bool,
}

impl CachedMarks {
    /// A source holding nothing yet, in its first frame.
    pub fn new() -> Self {
        let source = Self::default();
        source.recognitions.borrow_mut().begin_frame();
        source.payloads.borrow_mut().begin_frame();
        source
    }

    /// A source like [`CachedMarks::new`] whose series items draw their whole payloads.
    pub fn whole() -> Self {
        Self {
            whole: true,
            ..Self::new()
        }
    }

    /// Ends a frame and starts the next.
    pub fn end_frame(&self) {
        let mut recognitions = self.recognitions.borrow_mut();
        recognitions.end_frame();
        recognitions.begin_frame();
        let mut payloads = self.payloads.borrow_mut();
        payloads.end_frame();
        payloads.begin_frame();
    }

    /// Waits for every series payload build, and keeps what it built.
    pub fn settle(&self) {
        self.payloads.borrow_mut().settle();
    }
}

impl VectorMaskSource for CachedMarks {
    fn analytic(&self, _owner: VectorId) -> bool {
        false
    }

    fn path_glyphs(&self, _owner: VectorId) -> bool {
        false
    }

    fn recognitions(&self) -> Option<core::cell::RefMut<'_, super::recognitions::Recognitions>> {
        Some(self.recognitions.borrow_mut())
    }

    fn payloads(&self) -> Option<core::cell::RefMut<'_, super::payloads::MarkPayloads>> {
        Some(self.payloads.borrow_mut())
    }

    fn series_whole(&self) -> bool {
        self.whole
    }
}

/// The frames a route decline counts, and how long a history survives without a decline.
const HISTORY_FRAMES: u32 = 8;

/// The frames in which a large shape of one owner last failed each route.
#[derive(Clone, Copy, Debug, Default)]
struct Declines {
    /// The frame the history was last written in.
    frame: u32,
    /// The analytic route.
    analytic: Option<u32>,
    /// The marks route.
    marks: Option<u32>,
    /// The path glyph route.
    glyphs: Option<u32>,
}

/// The route declines of every shape drawn lately, and the caches the routes read.
#[derive(Debug, Default)]
pub(crate) struct VectorRouteCache {
    /// The declines of each owner.
    histories: FxHashMap<VectorId, Declines>,
    /// The current frame, advanced by [`VectorRouteCache::begin_frame`].
    frame: u32,
    /// What recognition found in the paths drawn lately.
    pub(crate) recognitions: super::recognitions::Recognitions,
    /// The mark payloads lowered lately.
    pub(crate) payloads: super::payloads::MarkPayloads,
    /// The splits of the paths drawn lately.
    pub(crate) splits: super::path_glyphs::Splits,
}

impl VectorRouteCache {
    /// Starts a frame.
    pub(crate) fn begin_frame(&mut self) {
        self.frame = self.frame.wrapping_add(1);
        self.recognitions.begin_frame();
        self.payloads.begin_frame();
        self.splits.begin_frame();
    }

    /// Ends a frame, forgetting the owners whose declines stopped counting.
    pub(crate) fn end_frame(&mut self) {
        let frame = self.frame;
        self.histories
            .retain(|_, history| frame.wrapping_sub(history.frame) < HISTORY_FRAMES);
        self.recognitions.end_frame();
        self.payloads.end_frame();
        self.splits.end_frame();
    }

    /// Whether `owner` may try the analytic route: false for [`HISTORY_FRAMES`] frames after it
    /// failed recognition.
    pub(crate) fn analytic_allowed(&self, owner: VectorId) -> bool {
        self.allowed(owner, |history| history.analytic)
    }

    /// Records that a large shape of `owner` failed recognition in this frame.
    pub(crate) fn note_analytic_declined(&mut self, owner: VectorId) {
        self.note(owner).analytic = Some(self.frame);
    }

    /// Whether `owner` may try the marks route: false for [`HISTORY_FRAMES`] frames after it
    /// failed recognition there.
    pub(crate) fn marks_allowed(&self, owner: VectorId) -> bool {
        self.allowed(owner, |history| history.marks)
    }

    /// Records that a large shape of `owner` failed recognition on the marks route in this
    /// frame.
    pub(crate) fn note_marks_declined(&mut self, owner: VectorId) {
        self.note(owner).marks = Some(self.frame);
    }

    /// Whether `owner` may try the path glyph route: false for [`HISTORY_FRAMES`] frames after a
    /// large shape of it failed to split.
    pub(crate) fn path_glyphs_allowed(&self, owner: VectorId) -> bool {
        self.allowed(owner, |history| history.glyphs)
    }

    /// Records that a large shape of `owner` failed to split into repeated outlines in this
    /// frame.
    pub(crate) fn note_path_glyphs_declined(&mut self, owner: VectorId) {
        self.note(owner).glyphs = Some(self.frame);
    }

    /// Forgets everything.
    pub(crate) fn clear(&mut self) {
        self.histories.clear();
        self.recognitions.clear();
        self.payloads.clear();
        self.splits.clear();
    }

    /// Whether the decline `of` reads from `owner`'s history stopped counting.
    fn allowed(&self, owner: VectorId, of: impl Fn(&Declines) -> Option<u32>) -> bool {
        self.histories
            .get(&owner)
            .and_then(of)
            .is_none_or(|declined| self.frame.wrapping_sub(declined) >= HISTORY_FRAMES)
    }

    /// Returns `owner`'s history, written in this frame.
    fn note(&mut self, owner: VectorId) -> &mut Declines {
        let frame = self.frame;
        let history = self.histories.entry(owner).or_default();
        history.frame = frame;
        history
    }
}

#[cfg(test)]
mod tests;
