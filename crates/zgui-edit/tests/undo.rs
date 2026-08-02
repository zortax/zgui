//! Undo, over ten thousand random editing sessions.
//!
//! Undo is where an editing model's mistakes accumulate rather than announce themselves: a range
//! recorded one byte out, a coalescing rule that joined two changes it should not have, a
//! selection restored from the wrong side of a deletion. Every one of those is correct for the
//! sequences somebody writes by hand and wrong for one sequence in a few thousand.
//!
//! The property asserted is the strong one. Every change that started a new undo entry has a state
//! it was made from — the text *and* the selection — and undoing back through the stack has to
//! reproduce each of those exactly, in order, ending at the state the session began in. Redoing
//! forwards again has to reproduce the state it was left in.

use proptest::prelude::*;
use zgui_edit::Editor;
use zgui_edit::editor::Command;
use zgui_edit::select::{Granularity, Motion, Selection};

/// The state an undo has to reproduce.
#[derive(Clone, Debug, PartialEq, Eq)]
struct State {
    /// What the text was.
    text: String,
    /// Where the caret or selection was.
    selection: Selection,
}

impl State {
    /// The editor's state right now.
    fn of(editor: &Editor) -> Self {
        Self {
            text: editor.text(),
            selection: editor.selection(),
        }
    }
}

/// One thing a session does.
#[derive(Clone, Debug)]
enum Step {
    /// Type some text.
    Type(String),
    /// Press backspace.
    Backspace,
    /// Press delete.
    Delete,
    /// Delete the word behind the caret.
    DeleteWord,
    /// Put the caret somewhere.
    Caret(usize),
    /// Select a range.
    Select(usize, usize),
    /// Select everything.
    SelectAll,
    /// Paste something.
    Paste(String),
    /// Move the caret one grapheme.
    Move(bool),
}

impl Step {
    /// Performs the step.
    fn run(&self, editor: &mut Editor) {
        let command = match self {
            Self::Type(text) => Command::Insert(text.clone()),
            Self::Backspace => Command::DeleteBackwards(Granularity::Grapheme),
            Self::Delete => Command::DeleteForwards(Granularity::Grapheme),
            Self::DeleteWord => Command::DeleteBackwards(Granularity::Word),
            Self::Caret(at) => Command::Select(Selection::caret(*at)),
            Self::Select(anchor, focus) => Command::Select(Selection::new(*anchor, *focus)),
            Self::SelectAll => Command::SelectAll,
            Self::Paste(text) => Command::Paste(text.clone()),
            Self::Move(forwards) => {
                Command::Move(Motion::new(Granularity::Grapheme, *forwards, false))
            }
        };
        editor.apply(command);
    }
}

/// Text a step types or pastes: short, and with the characters that break naive offsets in it.
fn text() -> impl Strategy<Value = String> {
    proptest::collection::vec(
        prop_oneof![
            Just('a'),
            Just(' '),
            Just('\n'),
            Just('é'),
            Just('日'),
            Just('_'),
            Just('.'),
        ],
        0..4,
    )
    .prop_map(|characters| characters.into_iter().collect())
}

/// One step of a session.
fn step() -> impl Strategy<Value = Step> {
    prop_oneof![
        text().prop_map(Step::Type),
        Just(Step::Backspace),
        Just(Step::Delete),
        Just(Step::DeleteWord),
        (0usize..24).prop_map(Step::Caret),
        (0usize..24, 0usize..24).prop_map(|(anchor, focus)| Step::Select(anchor, focus)),
        Just(Step::SelectAll),
        text().prop_map(Step::Paste),
        any::<bool>().prop_map(Step::Move),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 10_000,
        max_shrink_iters: 4_000,
        ..ProptestConfig::default()
    })]

    #[test]
    fn any_sequence_of_edits_undoes_back_through_every_state_it_passed_through(
        steps in proptest::collection::vec(step(), 0..12),
        start in text(),
    ) {
        let mut editor = Editor::new(&start);
        let initial = State::of(&editor);

        // The state before each change that started an entry, in the order the undos will come
        // back through. A change that was folded into the entry before it adds nothing: undoing
        // that entry goes back past both, which is the whole point of the folding.
        let mut boundaries = Vec::new();
        for step in &steps {
            let before = State::of(&editor);
            let entries = editor.history().len();
            step.run(&mut editor);
            if editor.history().len() > entries {
                boundaries.push(before);
            }
        }
        let finished = State::of(&editor);

        while let Some(expected) = boundaries.pop() {
            editor.apply(Command::Undo);
            prop_assert_eq!(State::of(&editor), expected);
        }
        // The text alone here: the loop above already asserted the selection at every boundary,
        // and a caret moved after the last change is not something an undo puts back — there is no
        // change to undo, and a person who moved the caret and pressed nothing expects it to stay.
        prop_assert_eq!(State::of(&editor).text, initial.text, "everything undone is where it began");

        // And forwards again: redo is only meaningful if it is the inverse of the undo above.
        for _ in 0..steps.len() {
            editor.apply(Command::Redo);
        }
        prop_assert_eq!(State::of(&editor).text, finished.text);
    }
}

#[test]
fn undoing_past_the_beginning_and_redoing_past_the_end_do_nothing() {
    let mut editor = Editor::new("abc");
    for _ in 0..4 {
        editor.apply(Command::Undo);
    }
    assert_eq!(editor.text(), "abc");
    for _ in 0..4 {
        editor.apply(Command::Redo);
    }
    assert_eq!(editor.text(), "abc");
}
