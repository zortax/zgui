//! The standing gate over how often a moving picture is drawn.
//!
//! An animation and an elastic edge both owe **one frame per refresh of the output they are on**,
//! and both owe it at sixty hertz, at seventy-five and at two hundred and forty. Nothing inside
//! either of them can say whether they got it: the values are interpolated against the clock, so
//! every frame that does run holds exactly the right value and every counter agrees with a motion
//! made of a third of the steps the output could have shown.
//!
//! # What this runs, and what it cannot
//!
//! The gate runs the two headless targets. Both mount a real window over the headless platform,
//! **put its surface on an output with a stated refresh rate**, and drive a virtual clock in
//! exact refresh intervals — so the rate is a real input to the runtime's pacing rather than a
//! constant the test also chose, and two hundred and forty hertz needs no two-hundred-and-forty
//! hertz display. What is counted is what the loop produced against the refreshes that elapsed.
//!
//! One half of the question is **not** here, and is not a gate:
//!
//! * whether the frames the loop ran reached the *device*. A frame whose picture is identical to
//!   the last one damages nothing, and a renderer refuses an undamaged frame rather than spending
//!   a swap-chain image on pixels the surface already holds — so a window can run a frame per
//!   refresh and present half as many. `scroll_cadence` closes most of that hole headlessly by
//!   counting *composed positions that differ* rather than frames, which is the quantity a
//!   presentation would be refused for. What remains is the compositor's own behaviour: the real
//!   swap chain, the real present, and a real output actually running at these rates.
//!
//! That last part needs a display server, a graphics device and a monitor in the mode concerned,
//! and none of the three exist on a machine running the definition of done. Wiring a version of it
//! into `ci` that "passes" would mean asserting something the run never measured, which is worse
//! than not asserting it. So it stays a **runnable probe**, invoked by hand:
//!
//! ```text
//! cargo run --release -p zgui-bench --bin anim-cadence   -- dev.zgui.anim   10
//! cargo run --release -p zgui-bench --bin scroll-cadence -- dev.zgui.scroll bottom
//! ```
//!
//! Both write a JSON report to the file named by `ZGUI_CADENCE_OUT`, and both open a window on the
//! desktop they are run from — which is why they are not run from a gate.

mod subject;

use std::path::Path;

use crate::error::Result;
use crate::gate;

/// Runs the gate.
pub(crate) fn run(root: &Path) -> Result<()> {
    gate::run(root, "cadence", subject::SUBJECTS)?;
    println!(
        "cadence          the loop's own cadence is settled above. Whether those frames reached a \
         real device is not: run `zgui-bench --bin anim-cadence` and `--bin scroll-cadence` on a \
         desktop for that, and read `xtask/src/cadence/mod.rs` for why it is not a gate."
    );
    Ok(())
}
