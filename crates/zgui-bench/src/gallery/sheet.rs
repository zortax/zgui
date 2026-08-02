//! The whole style sheet, and the rule that stops the animations which never end.

use crate::gallery::PROBE_SHEET;

thread_local! {
    /// Whether the document is mounted with its endless animations stopped.
    ///
    /// Read when the sheet is built, which is once per window, so it is set before the first window
    /// of a phase is opened and never changed after.
    pub(crate) static STILL: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// The rule that stops the animations which never end.
///
/// # Why a picture differential has to ask for it
///
/// The comparison is two photographs of one state: what the engine drew incrementally, and what a
/// full repaint of the same state draws. The repaint costs frames, and frames cost clock — the
/// paced frame that answers a full damage cannot be had without moving past its deadline. So the
/// two photographs are always a little apart in time, and anything on the page that is still moving
/// is in two places in them.
///
/// A transition ends, and the script waits for it. A `1600ms … infinite` pulse never does. Under it
/// every step reports a difference the size of one interval's worth of opacity, the noise floor
/// meant to bound it is another draw from the same distribution, and the real faults — a vector
/// pass short of an item, a document composed a hundred pixels from where it is scrolled to — are
/// filed among a dozen shimmering skeletons.
///
/// Stopping them costs the differential nothing it could have checked: the phase of a loop at a
/// moment neither photograph can pin down is not a fact about the engine. Everything else about the
/// animated elements — their boxes, their colours, their place in the document, the damage they
/// raise — is compared exactly as before.
/// Only keyframe animations, and every one of them: transitions still run, and the script still
/// waits each of them out, because a transition ends and can therefore be compared once it has.
pub(crate) const STILL_SHEET: &str =
    "\n*, *::before, *::after { animation-name: none !important; }";

/// The whole style sheet: the gallery's, with the probe row's added.
///
/// Setting `ZGUI_BENCH_NO_CURVES` adds a last rule that puts the three runs the gallery draws as
/// filled curves back into the glyph atlas. It is a diagnostic: a difference that survives it is a
/// difference the atlas path produces, and one that does not is the curve path's.
pub(crate) fn sheet() -> String {
    let mut sheet = format!("{}\n{PROBE_SHEET}", crate::shell::SHEET);
    if STILL.with(std::cell::Cell::get) {
        sheet.push_str(STILL_SHEET);
    }
    if std::env::var_os("ZGUI_BENCH_NO_CURVES").is_some() {
        sheet.push_str(
            "\n.turned-text { transform: none; }
             .display-text { font-size: 24px; }
             .gradient-text { --zgui-text-fill: foreground; background-image: none; }",
        );
    }
    sheet
}
