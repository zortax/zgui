//! The text fields: what is typed, where the caret goes, and what a form says about it.
//!
//! There is no caret in the document to look at. The insertion point belongs to the framework's
//! editing model, which is the only thing that knows where it is, and it is drawn over the lines
//! the frame laid out rather than placed as a box among them — so every question about it here is
//! asked of the model, as an offset into the field's own text. That is also the sharper question:
//! a click on the second row of a wrapped line and a click at the end of the first row land in the
//! same place on the screen and resolve to different offsets, and only the offset tells them
//! apart.

use zgui::geom::DevicePx;
use zgui::vocab::NamedKey;

use crate::script::find;
use crate::stage::Stage;
use crate::stage::census::{Census, Seen};

/// The text typed into the textarea, which has to wrap however wide the window opened.
///
/// Long enough that it cannot fit on one row of any window this runs in. A sentence that fits is a
/// sentence that never wrapped, and every claim about the row below the first is then a claim about
/// a row that is not there: the clicks all resolve to the only line, agree with each other, and
/// report a hit test that is answering perfectly correctly as broken.
const LONG: &str = "the quick brown fox jumps over the lazy dog and keeps running until it is out \
                    of sight, and then the dog gets up, shakes itself, and trots home along the \
                    towpath in the rain, past the boats and the bridge and the long wall with the \
                    ivy on it, without once looking back at where the fox went";

/// Drives the fields.
pub(crate) fn run(stage: &mut Stage<'_>) {
    typing(stage);
    wrapped_caret(stage);
    one_time_code(stage);
    form(stage);
}

/// The field whose whole text is `text`.
fn field_of<'a>(census: &'a Census, text: &str) -> Option<&'a Seen> {
    census.control(text)
}

/// Clicking into a field, typing into it, and leaving it again.
fn typing(stage: &mut Stage<'_>) {
    let Some((census, panel)) = find::open_panel(stage, "Input and textarea") else {
        stage
            .report
            .note("Input", "the Input panel is not laid out");
        return;
    };
    let Some(field) = field_of(&census, "Ada Lovelace").and_then(|node| node.rect) else {
        stage
            .report
            .note("Input", "the filled field is not laid out");
        return;
    };
    find::mark(stage, "field:name", field);

    // Click near the left of the text, which should put the caret before the first characters
    // rather than at the end of the value.
    stage.click(find::left_edge(field, 12.0, 0.5));
    stage.shot("fields-input-clicked");
    let placed = stage.caret();
    stage.report.check(
        "Input",
        "a click puts a caret in the field",
        placed.is_some(),
        &format!("the model reports {placed:?}"),
    );
    stage.report.check(
        "Input",
        "the click focuses the field",
        stage.focused_text().contains("Ada Lovelace"),
        &format!("focus is on {:?}", stage.focused_text()),
    );
    stage.report.check(
        "Input",
        "the caret lands where the click was, not at the end",
        placed.as_ref().is_some_and(|range| range.start <= 3),
        &format!(
            "the caret is at offset {:?} of twelve characters",
            placed.map(|range| range.start)
        ),
    );

    // Typing has to change what the field says, and the change has to be at the caret.
    stage.type_text("Zz");
    let after = stage.census();
    let value = after
        .node(
            stage
                .focused()
                .expect("the click focused the field, which was just checked"),
        )
        .map(|node| node.text.clone())
        .unwrap_or_default();
    stage.report.check(
        "Input",
        "typing reaches the field",
        value.contains("Zz"),
        &format!("the field now says {value:?}"),
    );
    stage.shot("fields-input-typed");

    // A field that has been left has to stop showing a caret, and keep its value.
    let elsewhere = find::left_edge(panel, 8.0, 0.02);
    stage.click(elsewhere);
    let after = stage.census();
    let still = after
        .nodes
        .iter()
        .any(|node| node.text.contains("Zz") && node.area() > 0.0);
    stage.report.check(
        "Input",
        "the value survives losing focus",
        still,
        &format!(
            "the typed text is {}",
            if still { "still there" } else { "gone" }
        ),
    );
    stage.shot("fields-input-blurred");

    // The placeholder is generated content, so there is no node here saying it — which is the
    // point: a field's element holds the text nodes the editing model writes and nothing else, and
    // that is what makes `:empty` mean "this field has no text". What can be asked here is the
    // condition the placeholder rule matches on. That the letters are then actually drawn is
    // asserted off the graphics device, in `zgui-ui/tests/typing.rs`.
    let census = stage.census();
    let empty_field = census
        .nodes
        .iter()
        .filter(|node| node.area() > 0.0)
        .find(|node| {
            node.rect.is_some_and(|rect| {
                rect.size.width.0 > 200.0 && (rect.size.height.0 - field.size.height.0).abs() < 2.0
            }) && node.text.is_empty()
        });
    stage.report.check(
        "Input",
        "the empty field holds no text of its own, which is what its placeholder rule matches on",
        empty_field.is_some(),
        &format!("the empty field is {:?}", empty_field.map(|node| node.rect)),
    );

    // A disabled field must refuse both the focus and the text.
    let census = stage.census();
    if let Some(locked) = census.control("Locked").and_then(|node| node.rect) {
        stage.click(find::left_edge(locked, 12.0, 0.5));
        stage.type_text("no");
        let after = stage.census();
        let took_it = after
            .inside(locked)
            .into_iter()
            .any(|node| node.text.contains("no") && node.text != "Locked");
        stage.report.check(
            "Input",
            "a disabled field takes neither focus nor text",
            !took_it && !stage.focused_text().contains("Locked"),
            &format!("focus is on {:?}", stage.focused_text()),
        );
    }
}

/// A line long enough to wrap, and a click on the left of the line it wrapped onto.
fn wrapped_caret(stage: &mut Stage<'_>) {
    let census = stage.census();
    let Some(area) = field_of(&census, "Two lines of\nnotes.")
        .or_else(|| {
            census
                .nodes
                .iter()
                .filter(|node| node.text.starts_with("Two lines of") && node.area() > 0.0)
                .min_by(|left, right| left.area().total_cmp(&right.area()))
        })
        .and_then(|node| node.rect)
    else {
        stage
            .report
            .note("Textarea", "the textarea is not laid out");
        return;
    };
    find::mark(stage, "field:notes", area);

    // Empty it and put one long line in, so that what is on the second row is there because it
    // wrapped rather than because a newline was typed.
    stage.click(find::left_edge(area, 12.0, 0.25));
    for _ in 0..40 {
        stage.key(NamedKey::Delete);
    }
    stage.key_with(NamedKey::End, zgui::vocab::Modifiers::NONE);
    for _ in 0..40 {
        stage.key(NamedKey::Backspace);
    }
    stage.type_text(LONG);
    stage.shot("fields-textarea-wrapped");

    let after = stage.census();
    let area = after
        .nodes
        .iter()
        .filter(|node| node.text.contains("quick brown") && node.area() > 0.0)
        .min_by(|left, right| left.area().total_cmp(&right.area()))
        .and_then(|node| node.rect)
        .unwrap_or(area);
    let typed = after
        .inside(area)
        .into_iter()
        .map(|node| node.text.clone())
        .max_by_key(String::len)
        .unwrap_or_default();
    stage.report.check(
        "Textarea",
        "a long line goes in",
        typed.contains("lazy dog"),
        &format!("{} characters are in the field", typed.len()),
    );

    // Two clicks at the same horizontal place, one near the top of the field and one near the
    // bottom, and the offsets they produce.
    //
    // Asked as a *pair* because neither click on its own says anything. How tall a row is and how
    // many rows this text wraps into are both decided by how wide the window opened, so a point
    // measured in assumed rows lands on the second row at one width and above the text at another,
    // where the answer is offset zero and the report is of a hit test that failed. What is true at
    // every width is that a click lower down the same field cannot resolve to an earlier character
    // than one higher up — and that if the line wrapped at all, the two are not the same character.
    let same_x = area.origin.x.0 + 40.0;
    let at = |fraction: f32| {
        zgui::geom::Point::new(
            DevicePx(same_x),
            DevicePx(area.origin.y.0 + area.size.height.0 * fraction),
        )
    };
    stage.click(at(0.2));
    let upper = stage.caret().map(|caret| caret.start);
    stage.click(at(0.8));
    stage.shot("fields-textarea-caret-second-line");
    let lower = stage.caret().map(|caret| caret.start);
    stage.report.check(
        "Textarea",
        "the line wrapped rather than ran off the edge",
        upper
            .zip(lower)
            .is_some_and(|(upper, lower)| upper != lower),
        &format!(
            "the field is {:.1} device pixels tall, holds {} characters, and one x resolves to \
             {upper:?} near its top and {lower:?} near its bottom",
            area.size.height.0,
            typed.len()
        ),
    );
    // The offset, which is the thing the two candidate answers differ in: the end of the first row
    // and the start of the second are the same place on the screen, and a hit test that resolved
    // the wrong one would put the next character at the end of the line above.
    stage.report.check(
        "Textarea",
        "a click on a wrapped row lands on that row rather than the end of the one above",
        upper
            .zip(lower)
            .is_some_and(|(upper, lower)| lower > upper && lower < typed.len()),
        &format!(
            "the caret went to {upper:?} and then to {lower:?} of {} characters, with the focus \
             on {:?}",
            typed.len(),
            stage.focused_text().chars().take(24).collect::<String>()
        ),
    );
}

/// The one-time code, which is one box per character.
fn one_time_code(stage: &mut Stage<'_>) {
    let Some((census, panel)) = find::open_panel(stage, "One-time code") else {
        stage.report.note("InputOtp", "the panel is not laid out");
        return;
    };
    let cells = census
        .inside(panel)
        .into_iter()
        .filter(|node| node.area() > 0.0 && node.text.chars().count() <= 1)
        .count();
    stage.report.check(
        "InputOtp",
        "six boxes are laid out",
        cells >= 6,
        &format!("{cells} boxes of at most one character"),
    );
    // The first box of the six-box demo, found by what it shows: the demo starts as "42", so its
    // first slot is the smallest thing in the panel saying exactly "4". A point a fraction of the
    // way down the panel is a photograph of a layout: the panel has three demos, and after a
    // rework three quarters down landed on the invalid demo's row label — which focuses nothing,
    // so the typed digits went nowhere and the boxes read back their pre-filled value.
    let Some(first) = find::at_in(&census, panel, "4") else {
        stage
            .report
            .note("InputOtp", "the six-box demo's first slot is not laid out");
        return;
    };
    stage.click(first);
    stage.type_text("135");
    stage.shot("fields-otp");
    // The panel is found again rather than reused: focusing a control scrolls it into view, so the
    // rectangle taken before the click names a place the boxes have since moved away from — and a
    // census read through it comes back empty however well the control works.
    let after = stage.census();
    let panel = after
        .panel("One-time code")
        .and_then(|node| node.rect)
        .unwrap_or(panel);
    let shown: String = after
        .inside(panel)
        .into_iter()
        .filter(|node| node.text.chars().count() == 1)
        .map(|node| node.text.clone())
        .collect();
    stage.report.check(
        "InputOtp",
        "typed digits land in the boxes",
        shown.contains('1') && shown.contains('3') && shown.contains('5'),
        &format!("the boxes say {shown:?}"),
    );
}

/// The form, and the one rule it holds.
fn form(stage: &mut Stage<'_>) {
    let Some((census, panel)) = find::open_panel(stage, "Form") else {
        stage.report.note("Form", "the panel is not laid out");
        return;
    };
    let Some(submit) = find::at_in(&census, panel, "Sign up") else {
        stage.report.note("Form", "no Sign up button");
        return;
    };

    stage.click(submit);
    stage.shot("fields-form-invalid");
    let after = stage.census();
    let complained = after
        .nodes
        .iter()
        .any(|node| node.text == "That does not look like an address." && node.area() > 0.0);
    stage.report.check(
        "Form",
        "submitting an invalid field says what is wrong",
        complained,
        "the field's own message is laid out",
    );

    // Make it valid and check the message goes away rather than staying for ever.
    let census = stage.census();
    if let Some(field) = census.control("not-an-address").and_then(|node| node.rect) {
        stage.click(find::left_edge(field, 12.0, 0.5));
        stage.key_with(NamedKey::End, zgui::vocab::Modifiers::NONE);
        stage.type_text("@example.com");
        stage.click(submit);
        stage.shot("fields-form-valid");
        let after = stage.census();
        let still = after
            .nodes
            .iter()
            .any(|node| node.text == "That does not look like an address." && node.area() > 0.0);
        stage.report.check(
            "Form",
            "the message goes when the field is put right",
            !still,
            if still {
                "the message is still on the screen"
            } else {
                "the message is gone"
            },
        );
    }
}
