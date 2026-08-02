//! A window that pushes tens of thousands of distinct glyphs past a label that never changes.
//!
//! # What the fixture has to produce
//!
//! The defect the eviction assertions exist for needs three things at once: something being
//! *replayed* rather than encoded, a great many *distinct* rasterisations passing through the
//! atlas beside it, and a budget small enough that the atlas frees something. Take any one away
//! and the situation is safe for a reason that has nothing to do with the fix.
//!
//! The fixed face gives one glyph per character with the character's own scalar value as its
//! index, so a paragraph of characters nothing has drawn before is a paragraph of atlas keys
//! nothing has allocated before, and 50 000 of them is 50 000 tiles.
//!
//! # Why the root's background is toggled
//!
//! A fragment the damage does not reach is not visited, and a fragment nothing visited loses its
//! record — correctly, since nothing is drawing it. So a label off to one side of a changing
//! paragraph is never replayed at all and the question cannot be asked. Toggling the root's own
//! background puts the whole surface in the damage every turn, so the label is visited, finds
//! nothing about itself changed, and replays.

use zgui_reactive::RwSignal;
use zgui_reactive::prelude::{Get, Set};
use zgui_reactive::reexport::LocalStorage;
use zgui_view::{AttrName, BuildCx, IntoView, View};

/// The label, the paragraph, and a root whose background follows an attribute.
const CSS: &str = ":root { display: block; width: 400px; height: 300px }
                   .root { display: block }
                   .root[data-flip=\"1\"] { background-color: rgb(16, 16, 16) }
                   text { display: block }";

/// How many distinct characters each turn puts on the page.
pub const PER_TURN: u32 = 250;

/// Where the characters are drawn from.
///
/// The supplementary ideographic plane: no surrogate halves inside it, so every step is a valid
/// scalar, and low enough in it that fifty thousand steps stay inside. The offset keeps the low
/// sixteen bits — which is what the fixed face uses as a glyph index — clear of the space
/// character, whose glyph is the one the face rasterises to no pixels at all.
const FIRST_SCALAR: u32 = 0x2_0100;

/// A window holding the fixture, and the signals that drive it.
pub struct Churn {
    /// The window and the loop around it.
    pub harness: zgui_platform_headless::Harness<zgui_runtime::Runtime>,
    /// The paragraph that is replaced every turn.
    text: RwSignal<String, LocalStorage>,
    /// What the root's `data-flip` attribute says, which is what damages the whole surface.
    flip: RwSignal<bool, LocalStorage>,
    /// How many distinct characters have been shown so far.
    shown: u32,
}

impl Churn {
    /// Opens the window, settles it, and holds it to `soft_bytes` of texture memory.
    ///
    /// `None` is a window with no budget at all, which is the control every non-vacuity assertion
    /// here needs: it must free nothing however much it rasterises.
    pub fn open(soft_bytes: Option<u64>) -> Self {
        let text = RwSignal::new_local(String::from("start"));
        let flip = RwSignal::new_local(false);
        let mut harness = crate::support::app_with_text(CSS, move |cx: &mut BuildCx<'_>| {
            Box::new(
                zgui_elements::column()
                    .class("root")
                    .attribute(AttrName::new("data-flip"), move || {
                        flip.get().then(|| "1".to_owned())
                    })
                    .child(zgui_elements::text().child("STATIC LABEL"))
                    .child(zgui_elements::text().child(move || text.get()))
                    .into_view()
                    .build(cx),
            )
        });
        harness.settle(16);
        {
            let window = &mut harness.app_mut().windows_mut()[0];
            window.set_verify_replays(true);
            window.content_mut().set_soft_bytes(soft_bytes);
        }
        Self {
            harness,
            text,
            flip,
            shown: 0,
        }
    }

    /// Runs one turn: a fresh paragraph of characters nothing has drawn before.
    pub fn turn(&mut self) {
        let mut fresh = String::with_capacity(PER_TURN as usize * 4);
        for _ in 0..PER_TURN {
            let scalar = FIRST_SCALAR + self.shown;
            self.shown += 1;
            fresh.push(char::from_u32(scalar).expect("the plane holds no surrogates"));
        }
        self.text.set(fresh);
        self.flip.set(self.shown.is_multiple_of(2 * PER_TURN));
        self.harness.settle(4);
    }

    /// How many distinct characters have been drawn since the window opened.
    pub fn shown(&self) -> u32 {
        self.shown
    }

    /// The window, for reading what it is holding.
    pub fn window(&self) -> &zgui_runtime::Window {
        &self.harness.app().windows()[0]
    }

    /// Fails if any live record names a raster the atlas no longer holds.
    ///
    /// # Panics
    ///
    /// Naming the first stale key. The walk's own check runs at the moment of each replay; this
    /// asks the same question between frames, of every record including those no fragment replayed
    /// this turn.
    pub fn assert_nothing_stale(&self, turn: usize) {
        let stale = self.window().stale_replay_resources();
        assert!(
            stale.is_empty(),
            "turn {turn}: {} raster(s) a live record names have been freed under it, the first \
             {:?}",
            stale.len(),
            stale.first()
        );
    }
}

/// What the counters did between `before` and now.
pub fn moved(before: &zgui_profile::Counters) -> zgui_profile::Counters {
    before.delta(&zgui_profile::counter::snapshot())
}
