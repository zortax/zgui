//! The second window a differential holds the first one against.
//!
//! One document mounted twice, at the same size, driven through the same script event for event.
//! One window is left alone; the other throws every held layout result away before each turn of its
//! loop, so it can be holding nothing when it is asked. Everything about arranging that is here,
//! because three differentials need it and each of them is about what is compared afterwards.

use zgui::geom::{Css, CssPx, DevicePx, Point, Size};
use zgui::platform::SurfaceEvent;
use zgui::runtime::Runtime;
use zgui_platform_headless::Harness;

use crate::gallery::{HEIGHT, Scheme, WIDTH, mounted_scheme, runtime};
use crate::input::focus_a_field;
use crate::inspect::swatch_centres;
use crate::script::{Driven, Step, run_step};

/// A cold window beside a live one, and what each of them is driven with.
pub(crate) struct Twin {
    /// The window that holds nothing.
    pub(crate) cold: Harness<Runtime>,
    /// How the live window is driven.
    live: Driven,
    /// How the cold one is.
    thorough: Driven,
}

impl Twin {
    /// Opens a second window on the same document at the same size, settled where the first is.
    ///
    /// # Panics
    ///
    /// Panics when the two windows did not open the same, which every comparison after it would
    /// otherwise report as a disagreement about the engine.
    pub(crate) fn open(
        size: &str,
        live: &mut Harness<Runtime>,
        scheme: Scheme,
        centres: &[Point<CssPx, Css>],
    ) -> Self {
        let mut cold = Harness::new(runtime(size));
        let cold_scheme = mounted_scheme();
        cold.deliver_to_first(SurfaceEvent::Resized(Size::new(
            DevicePx(WIDTH),
            DevicePx(HEIGHT),
        )));
        crate::verify::settle_cold(&mut cold, 256);
        let cold_centres = swatch_centres(&cold.app().windows()[0]);
        assert_eq!(&cold_centres, centres, "the two windows opened the same");
        let tabs = focus_a_field(live);
        assert_eq!(
            tabs,
            focus_a_field(&mut cold),
            "both windows reached a field in the same number of tabs"
        );
        Self {
            cold,
            live: Driven {
                cold: false,
                centres: centres.to_vec(),
                scheme,
            },
            thorough: Driven {
                cold: true,
                centres: cold_centres,
                scheme: cold_scheme,
            },
        }
    }

    /// Delivers one step of the script to both windows.
    pub(crate) fn step(&mut self, live: &mut Harness<Runtime>, step: &Step) {
        run_step(live, step, &self.live);
        run_step(&mut self.cold, step, &self.thorough);
    }

    /// Carries both windows to the moment neither of them owes anything.
    ///
    /// Two windows caught at quiet are not two windows in the same state: a transition part-way
    /// through its duration leaves a deadline behind and no work to do until it arrives, and the
    /// thorough window buys frames the other never asks for — so a movement sampled once per frame
    /// is sampled at different moments in each of them. Running both clocks down past every owed
    /// moment is the one state both can be in at once.
    pub(crate) fn settle(&mut self, live: &mut Harness<Runtime>) {
        crate::verify::run_down(live, false);
        live.settle(96);
        crate::verify::run_down(&mut self.cold, true);
        crate::verify::settle_cold(&mut self.cold, 96);
    }

    /// The window each side of the comparison is asked about.
    pub(crate) fn windows<'a>(
        &'a self,
        live: &'a Harness<Runtime>,
    ) -> (&'a zgui::runtime::Window, &'a zgui::runtime::Window) {
        (&live.app().windows()[0], &self.cold.app().windows()[0])
    }

    /// Whether the two windows laid the document out in the same place, to the bit.
    ///
    /// The control every comparison above layout needs. A hit answer and a published rectangle are
    /// both *derived* from where the fragments are, so two windows whose fragments are already a
    /// fraction of a pixel apart will differ in both — and reporting that as a hit-testing fault
    /// would attribute a layout difference to the stage that faithfully carried it. The steps where
    /// this answers `false` are the steps the byte-identity differential's own geometry half is
    /// reporting, so they are counted and named here rather than compared.
    pub(crate) fn laid_out_alike(&self, live: &Harness<Runtime>) -> bool {
        let (live_window, cold_window) = self.windows(live);
        crate::verify::geometry(live_window) == crate::verify::geometry(cold_window)
    }
}
