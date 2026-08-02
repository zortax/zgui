//! The call-and-block grammar, exercised form by form.
//!
//! Every case is written as **source text** rather than as a `quote!` invocation, because `quote!`
//! re-spaces the punctuation it emits and several of the forms below turn on spacing: `::` against
//! `:` `:`, and `>>` against `>` `>`.

use std::str::FromStr;

use proc_macro2::TokenStream;

/// Lowers one invocation written as source text, and normalises the whitespace out of it.
fn lower(source: &str) -> Result<String, String> {
    let tokens = TokenStream::from_str(source)
        .unwrap_or_else(|error| panic!("`{source}` is not a token stream: {error}"));
    super::expand(tokens)
        .map(|tokens| tokens.to_string().replace(' ', ""))
        .map_err(|error| error.to_string())
}

/// Lowers one invocation, which is expected to be well formed.
fn expand(source: &str) -> String {
    lower(source).unwrap_or_else(|error| panic!("`{source}` is well formed: {error}"))
}

/// The error one invocation produces, which is expected to have one.
fn error(source: &str) -> String {
    lower(source).expect_err("this view is not well formed")
}

#[test]
fn the_first_token_tree_decides_which_node_is_written() {
    assert_eq!(expand("row()"), "::zgui::expansion::elements::row()");
    assert_eq!(expand("\"a\""), "\"a\"");
    assert_eq!(expand("{count}"), "count");
}

#[test]
fn an_empty_attribute_list_may_be_elided() {
    assert_eq!(expand("label()"), expand("label {}"));
    assert_eq!(expand("label() {}"), expand("label {}"));
    assert_eq!(expand("label(){\"a\"}"), expand("label { \"a\" }"));
}

#[test]
fn a_name_on_its_own_is_not_a_node_and_the_error_says_what_one_is() {
    let message = error("spacer");
    assert!(
        message.contains("`spacer` is a name, not a node"),
        "{message}"
    );
    assert!(message.contains("`spacer()`"), "{message}");
    assert!(message.contains("`{spacer}`"), "{message}");
    assert!(message.contains("`\"spacer\"`"), "{message}");
}

/// The advice a plain name gets — call it, and it is a childless element — resolves to an element
/// of that name, and there is no `format` element. So a macro call is told the truth instead.
#[test]
fn a_macro_call_in_node_position_is_told_it_is_a_value() {
    let message = error("column { format!(\"{n} left\") }");
    assert!(message.contains("macro call"), "{message}");
    assert!(message.contains("{format!(…)}"), "{message}");
}

#[test]
fn a_bare_expression_is_not_a_child() {
    // A name begins a call, so an expression that starts with one is told it wrote a name where a
    // node belongs; anything else is told that a node was expected at all.
    assert!(error("column { item.label.clone() }").contains("`item` is a name, not a node"));
    assert!(error("column { 42 }").contains("expected a node"));
    assert!(error("column { \"a\".len() }").contains("expected a node"));
}

#[test]
fn a_head_may_be_a_path_a_keyword_or_hyphenated() {
    assert_eq!(expand("html::div()"), "html::div()");
    assert_eq!(expand("box()"), "::zgui::expansion::elements::r#box()");
    assert_eq!(
        expand("overlay-root()"),
        "::zgui::expansion::elements::overlay_root()"
    );
}

#[test]
fn the_words_control_flow_uses_may_not_name_a_node() {
    for reserved in ["while", "loop", "match"] {
        let message = error(&format!("{reserved}() {{}}"));
        assert!(
            message.contains("has no meaning in a view"),
            "`{reserved}`: {message}"
        );
        assert!(
            message.contains("`for` and `if` are the control flow a view has"),
            "`{reserved}`: {message}"
        );
    }
    // The two words that belong to a keyword are told which one.
    assert!(error("else() {}").contains("after the block of an `if`"));
    assert!(error("in() {}").contains("after the row of a `for`"));
    // The components they are sugar for keep their own names.
    assert!(lower("For(each = items, let:item) { {item} }").is_ok());
}

#[test]
fn a_node_takes_one_attribute_list_and_it_comes_first() {
    let message = error("row() (class = \"a\")");
    assert!(message.contains("`(` cannot begin a node"), "{message}");
}

#[test]
fn a_tag_is_not_a_node() {
    let message = error("<row/>");
    assert!(message.contains("`<` cannot begin a node"), "{message}");
    assert!(message.contains("Button(class = \"x\")"), "{message}");
}

#[test]
fn children_are_juxtaposed_and_nest() {
    assert_eq!(
        expand("row { \"a\" {b} column() }"),
        "::zgui::expansion::elements::row().child(\"a\").child(b)\
         .child(::zgui::expansion::elements::column())"
    );
    assert!(expand("row { column { \"a\" } }").contains("column().child(\"a\")"));
    let fragment = expand("a() b()");
    assert!(fragment.starts_with('('), "{fragment}");
    assert_eq!(fragment.matches("into_view(").count(), 2, "{fragment}");
    assert_eq!(expand(""), "()");
}

#[test]
fn an_expression_child_keeps_its_braces() {
    assert!(
        expand("text(class = \"x\") { {item.label.clone()} }")
            .contains(".child(item.label.clone())")
    );
    assert!(error("row { {a} {b} extra }").contains("`extra` is a name, not a node"));
    assert!(error("row { {a b} }").contains("a braced child is one expression"));
}

/// A `{` after an attribute list is that call's children whatever stands between them, because a
/// macro cannot see whitespace. So a childless call followed by a braced sibling adopts it, and
/// the empty block — which expands to exactly what the childless call does — is what keeps the two
/// apart.
#[test]
fn an_empty_children_block_keeps_a_braced_sibling_a_sibling() {
    let adopted = expand("vector(class = \"axes\") {\"x\"}");
    assert!(
        adopted.contains("vector().class(\"axes\").child(\"x\")"),
        "{adopted}"
    );

    let sibling = expand("vector(class = \"axes\") {} {\"x\"}");
    assert!(
        sibling.contains("into_view(::zgui::expansion::elements::vector().class(\"axes\"))"),
        "{sibling}"
    );
    assert!(sibling.contains("into_view(\"x\")"), "{sibling}");

    // A braced sibling whose content is an expression rather than a node is adopted too, and the
    // expression is then read as a node list: `{move || ticks()}` written next to a childless call
    // is a parse error at `move`, not a child of it.
    assert!(error("vector(class = \"axes\") {move || ticks()}").contains("not a node"));
    assert!(expand("vector(class = \"axes\") {} {move || ticks()}").contains("move||ticks()"));
}

#[test]
fn attributes_are_separated_by_commas_and_may_carry_a_trailing_one() {
    let lowered = expand("row(class = \"a\", on:click = h,)");
    assert!(lowered.contains(".class(\"a\")"), "{lowered}");
    assert!(
        lowered.contains(".on(::zgui::expansion::view::events::CLICK,h)"),
        "{lowered}"
    );
    // A name with no value is still the shorthand it always was, so a comma written where a
    // parenthesis was meant is two attributes rather than one.
    assert_eq!(
        expand("row(flag = a, b)"),
        "::zgui::expansion::elements::row().flag(a).b(b)"
    );
}

#[test]
fn a_component_s_class_props_are_merged_rather_than_replacing_each_other() {
    // A props builder's setter stores over what an earlier call stored, so lowering `class = A,
    // class = B` as two calls keeps only B — and that spelling is every wrapper component adding
    // its own class before the caller's.
    let lowered = expand("Input(class = \"zui-input-group__field\", class = class)");
    assert_eq!(lowered.matches(".class(").count(), 1, "{lowered}");
    assert!(
        lowered.contains(
            ".class(::zgui::expansion::view::Classes::from(\"zui-input-group__field\")\
             .merged(&::zgui::expansion::view::Classes::from(class)))"
        ),
        "{lowered}"
    );

    // One class alone is handed to the setter as it was written, so a caller's own
    // `impl Into<Classes>` value is not wrapped in a conversion it did not ask for.
    let single = expand("Input(class = class)");
    assert!(single.contains(".class(class)"), "{single}");
}

#[test]
fn a_comma_inside_a_value_belongs_to_the_value() {
    let lowered = expand("For(key = |a: &Todo, b: &Todo| a.id > b.id, each = items) { \"x\" }");
    assert!(lowered.contains("|a:&Todo,b:&Todo|a.id>b.id"), "{lowered}");
    assert!(lowered.contains(".each(items)"), "{lowered}");
    assert!(expand("row(at = (x, y))").contains(".at((x,y))"));
}

#[test]
fn a_braced_value_expands_to_what_was_written_inside_it() {
    assert_eq!(expand("row(class = {class})"), expand("row(class = class)"));
    assert!(expand("row(hidden = {flag})").contains(".hidden(flag)"));
}

/// A value is one expression and a brace after it opens a block rather than a functional update,
/// so a comma that went missing before a spread is a parse error rather than a struct literal.
#[test]
fn a_struct_literal_value_is_written_in_braces() {
    let message = error("row(at = Point { x, y })");
    assert!(message.contains("a struct literal"), "{message}");
    assert!(message.contains("`at = {Point { … }}`"), "{message}");
    assert!(expand("row(at = {Point { x, y }})").contains(".at(Point{x,y})"));
}

/// A comma is the only thing that says the next attribute has begun, so the one that went missing
/// is named where it should have been written rather than left to the expression parser.
#[test]
fn attributes_written_with_no_comma_between_them_say_which_one_is_missing() {
    for source in [
        "row(class = \"a\" hidden)",
        "row(class = \"a\" on:click = h)",
        "Button({..attrs} {..more})",
    ] {
        let message = error(source);
        assert!(
            message.contains("attributes are separated by commas"),
            "{source}: {message}"
        );
    }
    // The comma that went missing before a spread leaves the same shape behind as a struct
    // literal, and it is a different mistake.
    let message = error("Button(class = \"a\" {..attrs})");
    assert!(!message.contains("struct literal"), "{message}");
}

/// A bundle written among the children is a range expression, which parses and then fails against
/// a trait naming a type nobody wrote. Both places it can be written are refused instead.
#[test]
fn a_spread_among_the_children_names_the_attribute_list() {
    for source in ["Card(class = \"c\") {..attrs}", "Card { {..attrs} }"] {
        let message = error(source);
        assert!(
            message.contains("goes in the attribute list"),
            "{source}: {message}"
        );
        assert!(message.contains("`Card({..attrs})`"), "{source}: {message}");
    }
}

/// Every expression the cut rule could not read, because `>` closed the tag it was written in.
#[test]
fn a_value_may_be_any_expression_rust_admits() {
    assert!(expand("row(state:open = count.get() > 0)").contains("count.get()>0"));
    assert!(expand("row(class:over = move || items.get().len() > 10)").contains("len()>10"));
    assert!(expand("row(prop:mask = bits >> 2)").contains("bits>>2"));
    assert!(
        expand("row(on:click = move |ev: &mut EventCx<'_, Click>| f(ev))")
            .contains("move|ev:&mutEventCx<'_,Click>|f(ev)")
    );
    assert!(expand("row(f = |x| -> Vec<u8> { x })").contains("|x|->Vec<u8>{x}"));
    assert!(expand("row(n = value as Wrapping<u8>)").contains("valueasWrapping<u8>"));
    assert!(expand("row(x = if a > b { p } else { q })").contains("ifa>b{p}else{q}"));
    assert!(
        expand("Each(each = move || xs.get().into_iter().collect::<Vec<_>>()) { {x} }")
            .contains("collect::<Vec<_>>()")
    );
}

#[test]
fn a_namespaced_name_keeps_its_colon_and_its_hyphens() {
    let lowered = expand(
        "row(class:active = on, style:gap = \"1rem\", var:--brand = \"red\", \
         attr:data-testid = \"row\", prop:value = v, state:disabled, custom_state:picked = p, \
         a11y:label = \"Save\", node_ref = handle)",
    );
    assert!(lowered.contains("ClassName::new(\"active\")"), "{lowered}");
    assert!(
        lowered.contains(".style_property(\"gap\",\"1rem\")"),
        "{lowered}"
    );
    assert!(
        lowered.contains("CustomPropertyName::new(\"brand\")"),
        "{lowered}"
    );
    assert!(
        lowered.contains("AttrName::new(\"data-testid\")"),
        "{lowered}"
    );
    assert!(lowered.contains("PropKey::new(\"value\")"), "{lowered}");
    assert!(lowered.contains("UiState::DISABLED,true"), "{lowered}");
    assert!(lowered.contains("Ident::new(\"picked\")"), "{lowered}");
    assert!(lowered.contains(".label(\"Save\")"), "{lowered}");
    assert!(lowered.contains(".node_ref(handle)"), "{lowered}");
}

#[test]
fn a_listener_keeps_its_modifiers() {
    let lowered = expand("row(on:click:capture:once = h)");
    assert!(lowered.contains("capture:true"), "{lowered}");
    assert!(lowered.contains("once:true"), "{lowered}");
    assert!(error("row(on:click)").contains("needs a handler"));
}

#[test]
fn a_spread_is_replayed_where_it_is_written() {
    let lowered = expand("Button(class:mine = true, {..attrs}, attr:data-x = \"1\")");
    let bundle = lowered.split(".attrs(").nth(1).expect("there is a bundle");
    let position = |needle: &str| bundle.find(needle).expect("the entry is there");
    assert!(position(".class_toggle(") < position(".merged(attrs)"));
    assert!(position(".merged(attrs)") < position(".attribute("));
    let message = error("Button({attrs})");
    assert!(message.contains("{..attrs}"), "{message}");
    assert!(message.contains("not in the parentheses"), "{message}");
}

#[test]
fn a_slot_child_fills_the_prop_it_names() {
    let lowered = expand("Card { CardHeader(slot) { \"h\" } \"body\" }");
    assert!(
        lowered.contains(".card_header(CardHeader::builder().children("),
        "{lowered}"
    );
    assert!(
        expand("Card { CardHeader(slot = \"header\") {} }")
            .contains(".header(CardHeader::builder()"),
    );
}

#[test]
fn a_let_binding_names_the_argument_the_children_closure_takes() {
    let lowered = expand("Each(each = items, let:item) { {item} }");
    assert!(
        lowered.contains(".children(move|item|::zgui::expansion::view::AnyView::new(item))"),
        "{lowered}"
    );
}

/// The spelling the grammar replaced is not read at all: a `<` begins no node, so a view written
/// the old way is a diagnostic rather than a second front end nobody maintains.
#[test]
fn the_spelling_this_grammar_replaced_is_not_read() {
    let message = error("<row class=\"a\">\"hi\"</row>");
    assert!(message.contains("`<` cannot begin a node"), "{message}");
}

/// The element vocabulary is not a dependency of this crate, so the messages the element lowering
/// raises have no fixture to be checked in beside; this one is asserted here in its stead.
#[test]
fn a_custom_property_is_named_with_the_dashes_it_is_declared_with() {
    let message = error("row(var:brand = \"red\")");
    assert!(message.contains("starts with `--`"), "{message}");
    assert!(message.contains("`var:--brand=…`"), "{message}");
    assert!(expand("row(var:--brand = \"red\")").contains("CustomPropertyName::new(\"brand\")"));
}

/// A keyword is sugar and nothing else, so each shape is lowered beside the call it stands for and
/// the two token streams are compared. A drift between the two fails here rather than in an
/// application, and the call spelling stays available for the heads a keyword cannot accept.
#[test]
fn a_list_lowers_to_the_call_it_is_sugar_for() {
    assert_eq!(
        expand("for item in move || items.get(), key = |item: &Todo| item.id { {item} }"),
        expand("For(each = move || items.get(), key = |item: &Todo| item.id, let:item) { {item} }")
    );
    // The head is copied across verbatim, so a closure written without `move` stays without one.
    assert_eq!(
        expand("for n in || 0..10, key = |n: &usize| *n { {n} }"),
        expand("For(each = || 0..10, key = |n: &usize| *n, let:n) { {n} }")
    );
}

#[test]
fn a_conditional_lowers_to_the_call_it_is_sugar_for() {
    assert_eq!(
        expand("if move || open.get() { \"a\" }"),
        expand("Show(when = move || open.get()) { \"a\" }")
    );
    assert_eq!(
        expand("if move || open.get() { \"a\" } else { }"),
        expand("Show(when = move || open.get(), fallback = || ()) { \"a\" }")
    );
    assert_eq!(
        expand("if move || open.get() { \"a\" } else { label() { \"b\" } }"),
        expand(
            "Show(when = move || open.get(), fallback = move || view! { label() { \"b\" } }) \
             { \"a\" }"
        )
    );
}

/// The key is written with the value production every attribute uses, so a braced one is unwrapped
/// and a comma inside a closure belongs to the closure.
#[test]
fn a_key_is_a_value_like_any_other() {
    assert_eq!(
        expand("for row in move || rows.get(), key = {|row: &Row| row.id} { {row} }"),
        expand("for row in move || rows.get(), key = |row: &Row| row.id { {row} }")
    );
    let lowered = expand("for at in move || gaps.get(), key = |a: &(A, B)| a.0, { {at} }");
    assert!(lowered.contains(".key(|a:&(A,B)|a.0)"), "{lowered}");
}

/// Control flow nests in itself and in everything else, because it is a node.
#[test]
fn control_flow_is_a_node_and_nests_wherever_one_does() {
    let lowered = expand(
        "column { for row in move || rows.get(), key = |r: &R| r.id { \
         if move || row.on.get() { text() { {row.label} } } } }",
    );
    assert!(lowered.contains("ForProps::builder()"), "{lowered}");
    assert!(lowered.contains("ShowProps::builder()"), "{lowered}");
    assert!(
        lowered.contains("::zgui::expansion::elements::column()"),
        "{lowered}"
    );
}

/// The head is a closure by token, checked before a single token of it is consumed, so the reading
/// that is read once has no spelling at all.
#[test]
fn a_head_that_is_not_a_closure_is_a_parse_error_on_the_head() {
    let message = error("for item in items.get(), key = |i: &T| i.id { {item} }");
    assert!(message.contains("`in` takes a closure"), "{message}");
    assert!(message.contains("`in move || items.get()`"), "{message}");

    let message = error("for n in 0..10, key = |n: &usize| *n { {n} }");
    assert!(message.contains("not Rust's `for`"), "{message}");

    let message = error("if open.get() { \"a\" }");
    assert!(message.contains("`if` takes a closure"), "{message}");
    assert!(message.contains("`if move || open.get()`"), "{message}");

    // A bare name may already hold a closure, and that is the one head with a component spelling
    // and no keyword.
    let message = error("if chosen { \"a\" }");
    assert!(message.contains("`Show(when = chosen) { … }`"), "{message}");
}

#[test]
fn control_flow_says_what_it_is_missing() {
    assert!(error("for item in move || items.get() { {item} }").contains("a list needs a key"));
    assert!(
        error("for (at, gap) in move || g.get(), key = k { \"a\" }").contains("binds one name")
    );
    assert!(
        error("for i in move || g.get(), key = k { } ").contains("a `for` needs a row"),
        "an empty row is refused"
    );
    assert!(error("if move || open.get() { }").contains("an `if` needs a body"));
    assert!(
        error("if move || open.get() { \"a\" } else if move || b.get() { \"b\" }")
            .contains("`else` takes a block")
    );
    assert!(
        error("for i in move || g.get(), key = k { \"a\" } else { \"b\" }")
            .contains("`for` has no `else`")
    );
    assert!(
        error("for i in move || g.get(), key = k, (class = \"x\") { \"a\" }")
            .contains("control flow takes no attributes")
    );
    assert!(error("if let Some(x) = y { \"a\" }").contains("`if let` is not part of"));
}

/// A conditional resolves a name the author did not write, so the two errors that arrive before
/// the resolver does say which name it is.
#[test]
fn a_conditional_names_the_component_it_needs_in_scope() {
    assert!(error("if open.get() { \"a\" }").contains("needs `Show` in scope"));
    assert!(error("if move || open.get() { }").contains("needs `Show` in scope"));
}
