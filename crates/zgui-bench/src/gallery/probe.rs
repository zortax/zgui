//! The probe row: the one thing in the document that is not the gallery's own.

use zgui::prelude::*;
use zgui::reactive::RwSignal;
use zgui::{component, view};

/// A box stroked with a ramp rather than filled with one.
///
/// The stroke arm and the fill arm are different code paths all the way down, and the fill arm
/// passing on its own is how a defect in the other one stays invisible. This is the smallest
/// drawing that exercises it: one rectangle, one ramp, painted along the outline.
const STROKED: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64">
  <defs>
    <linearGradient id="ramp" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="#7ee3ff"/>
      <stop offset="0.55" stop-color="#2f6bff"/>
      <stop offset="1" stop-color="#d18bff"/>
    </linearGradient>
  </defs>
  <rect x="6" y="6" width="52" height="52" rx="10" fill="none"
        stroke="url(#ramp)" stroke-width="8"/>
</svg>"##;

/// The probe row: four swatches whose only job is to be clicked, and a scroll port whose only job
/// is to carry a gradient past its own boundary.
///
/// The same four swatches the `styled` gallery has, at the same size and with the same handler, so
/// that the one interaction measured at every document size is identical at every document size.
///
/// The port below them is the other half, and it is here because a differential can only find a
/// defect in a document that contains one. A gradient is anchored where it is *painted*, so a box
/// carrying one past a scroll boundary is the case in which "where the box is" and "where the ramp
/// is sampled" can come apart — and every differential this harness runs was, until this row
/// existed, over documents in which no gradient ever moved.
///
/// # Why the port is named, and why it has to be smaller than the window
///
/// The gallery's own scroller is the **root** one, and a root scroller is the case every
/// instrument here already covered. Content below a root port is below the *surface* too, so the
/// frame cuts its damage away, the emit walk never visits it while it is out of view, and whatever
/// it was last told about itself is dropped rather than carried. An inner port is the other case
/// entirely: its rows are off the port and still on the surface, so they are visited on every frame
/// of a scroll while painting nothing at all, and anything the walk concludes about them there
/// travels with them into view.
///
/// So this one carries a `data-testid` and the script wheels over it by name. Six bands in
/// ninety-six pixels of port is four screenfuls of content that has never been composed where it is
/// about to arrive, in a document where nothing animates — which is the state in which arriving
/// content is drawn by whatever the walk decided about it while it was hidden, with nothing
/// repainting over the answer.
#[component]
pub(crate) fn Probe() -> impl IntoView {
    let picked = RwSignal::new(2_usize);
    view! {
        column(class = "probe-stack") {
            row(class = "probe") {
                for index in || [0_usize, 1, 2, 3], key = |index: &usize| *index {
                    control(
                        class = "swatch",
                        class:swatch-picked = move || picked.get() == index,
                        a11y:label = "Swatch",
                        on:click = move |_| picked.set(index)
                    )
                }
            }
            box(class = "probe-port", attr:data-testid = "probe-port") {
                column(class = "probe-tall") {
                    for _band in || [0_usize, 1, 2, 3, 4, 5], key = |index: &usize| *index {
                        row(class = "probe-band") {
                            box(class = "probe-ramp")
                            vector(
                                class = "probe-stroke",
                                prop:svg = STROKED,
                                a11y:label = "A box stroked with a ramp"
                            )
                            box(class = "probe-ramp probe-ramp-radial")
                        }
                    }
                }
            }
        }
    }
}

/// What the probe row looks like, added over the gallery's own sheet.
pub(crate) const PROBE_SHEET: &str = zgui::css!(
    ".probe { gap: 10px; }
     .swatch {
        width: 34px;
        height: 34px;
        border-radius: 10px;
        border: 2px solid transparent;
        background-color: #232b3a;
     }
     .swatch:hover { background-color: #2c3546; }
     .swatch-picked { border-color: #7ee3ff; background-color: #2f6bff; }

     .probe-stack { gap: 10px; }
     .probe-port {
        width: 320px;
        height: 96px;
        overflow-y: scroll;
        border-radius: 8px;
     }
     .probe-tall { gap: 8px; }
     .probe-band { gap: 8px; height: 64px; }
     .probe-ramp {
        width: 96px;
        height: 64px;
        border-radius: 8px;
        background-image: linear-gradient(100deg, #7ee3ff, #7d8bff 55%, #d18bff);
     }
     .probe-ramp-radial {
        background-image: radial-gradient(circle at 30% 30%, #24d3a5, #2f6bff 60%, #12161e);
     }
     .probe-stroke { width: 64px; height: 64px; }"
);
