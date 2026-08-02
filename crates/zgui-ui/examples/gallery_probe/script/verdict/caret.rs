//! Whether a field somebody has just emptied still shows where the next letter would go.
//!
//! The field looked at is the one with no placeholder, because a placeholder is drawn by a rule that
//! matches an empty field and therefore leaves ink of its own in the box: a field that still has
//! something to draw when its value is gone cannot answer the question. This one has nothing.
//!
//! There is no caret in the document to point a census at — the insertion point belongs to the
//! editing model and is drawn over the lines the frame laid out — so the model's own answer is
//! recorded, and the pictures are taken across several half-periods of the blink. A caret that is
//! there says so by being absent from half of them.

use core::time::Duration;

use zgui::vocab::NamedKey;

use crate::script::find;
use crate::script::gauntlet::ink::shot_of;
use crate::script::verdict;
use crate::stage::Stage;

/// What the field says before it is emptied.
const FILLED: &str = "Ada Lovelace";

/// How many pictures to take of the emptied field.
///
/// The blink turns over twice a second and stops after ten seconds of stillness, so this many
/// captures at the rate one costs span several turns and finish well inside that window.
const SAMPLES: usize = 16;

/// How long to leave between two captures, on top of what a capture costs.
const APART: Duration = Duration::from_millis(45);

/// How much room to leave around the field in the pictures, in device pixels.
const MARGIN: f32 = 3.0;

/// Empties the field with no placeholder and photographs it blinking.
pub(crate) fn run(stage: &mut Stage<'_>) {
    let Some((census, _panel)) = find::open_panel(stage, "Input and textarea") else {
        stage
            .report
            .note("Input", "the Input panel is not laid out");
        return;
    };
    let Some(field) = census.control(FILLED).and_then(|node| node.rect) else {
        stage
            .report
            .note("Input", "the field with no placeholder is not laid out");
        return;
    };
    find::mark(stage, "field:no-placeholder", field);

    stage.click(find::left_edge(field, 12.0, 0.5));
    stage.key(NamedKey::End);
    for _ in 0..FILLED.chars().count() {
        stage.key(NamedKey::Backspace);
    }

    let Some(focused) = stage.focused() else {
        stage
            .report
            .note("Input", "nothing is focused after the field was emptied");
        return;
    };
    let says = stage.handles().dom.text_content(focused);
    stage.report.check(
        "Input",
        "the field is empty and still focused",
        says.is_empty(),
        &format!("it says {says:?}"),
    );
    stage.report.check(
        "Input",
        "the model puts the caret at the start of the emptied field",
        stage.caret() == Some(0..0),
        &format!("the model reports {:?}", stage.caret()),
    );

    // The pictures. Nothing else in the window is touched between them, so the only thing that can
    // differ from one to the next is what the blink did.
    for sample in 0..SAMPLES {
        shot_of(
            stage,
            &format!("vd-caret-{sample:02}"),
            verdict::grown(field, MARGIN),
        );
        stage.wait(APART);
    }
    stage.report.note(
        "Input",
        &format!("{SAMPLES} pictures of the emptied field, {APART:?} of extra time apart"),
    );
}
