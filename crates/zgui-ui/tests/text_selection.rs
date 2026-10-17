//! Input and Textarea inherit shared multi-click selection without component handlers.
mod desktop;

use desktop::stage::Stage;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use zgui::geom::{DevicePx, Point};
use zgui::prelude::*;
use zgui_ui::prelude::*;
use zgui_ui_tokens::prelude::*;

#[test]
fn controlled_input_and_uncontrolled_textarea_select_words_and_lines() {
    let built = Rc::new(RefCell::new(Vec::new()));
    let model = Rc::clone(&built);
    let changes = Rc::new(Cell::new(0));
    let changed = Rc::clone(&changes);
    let mut stage = Stage::open(
        ".page {padding:24px;gap:20px;align-items:flex-start} .field {width:320px}",
        move || {
            let input = NodeRef::new();
            let textarea = NodeRef::new();
            *model.borrow_mut() = vec![input, textarea];
            let value = RwSignal::new_local("alpha beta".to_owned());
            let input_change = Rc::clone(&changed);
            let textarea_change = Rc::clone(&changed);
            view! { ThemeProvider { column(class="page") {
                Input(node_ref=input,class="field",value=value,label="Name",on_change=UnsyncCallback::new(move |_| input_change.set(input_change.get()+1)))
                Textarea(node_ref=textarea,class="field",default_value="alpha beta\nnext line".to_owned(),label="Notes",on_change=UnsyncCallback::new(move |_| textarea_change.set(textarea_change.get()+1)))
            } } }
        },
    );
    for (index, node) in built.borrow().iter().copied().enumerate() {
        let rect = node.window_bounds().unwrap();
        let point = Point::new(
            DevicePx(rect.origin.x.0 + rect.size.width.0 - 12.0),
            DevicePx(rect.origin.y.0 + rect.size.height.0 - 12.0),
        );
        let (end, word, line) = if index == 0 {
            (10, 6..10, 0..10)
        } else {
            (20, 16..20, 11..20)
        };
        stage.click(point);
        assert_eq!(node.selection(), Some(end..end));
        stage.click(point);
        assert_eq!(node.selection(), Some(word));
        stage.click(point);
        assert_eq!(node.selection(), Some(line));
    }
    assert_eq!(
        changes.get(),
        0,
        "Selecting text must not report value edits"
    );
}
