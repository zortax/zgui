//! The authoring surface: `view!`, `#[component]`, `#[slot]`, `variants!`, `css!` and `style!`.
//!
//! A user interface is written as nested calls with typed attributes, and a component is written
//! as an ordinary function. Neither is a new language: `view!` expands to a builder chain, and
//! `#[component]` expands to a function, a props struct and a builder for it. The types do the
//! work, so an editor completes attribute names, jumps to their documentation, and reports a
//! mistake against the thing it names.
//!
//! ```
//! # extern crate zgui_view as zgui;
//! use zgui_view::prelude::*;
//! use zgui_view_macro::{component, view};
//!
//! /// A greeting.
//! #[component]
//! fn Greeting(
//!     /// Who is greeted.
//!     #[prop(into)]
//!     name: String,
//! ) -> impl IntoView {
//!     view! { "Hello, "{name}"!" }
//! }
//!
//! let greeting = view! { Greeting(name = "world") };
//! ```
//!
//! # The grammar
//!
//! ```text
//! view!     := node*                          more than one root is a fragment
//! node      := call | text | block | list | branch
//! call      := head (attrs children? | children)   at least one of the two
//! head      := ident ('-' ident)*             the intrinsic vocabulary
//!            | path '::' ident                someone else's vocabulary
//!            | Path                           a component
//! attrs     := '(' (attr (',' attr)* ','?)? ')'
//! children  := '{' node* '}'
//! text      := string-literal
//! block     := '{' rust-expression '}'        anything that converts into a view
//! list      := 'for' ident 'in' thunk ',' 'key' '=' value ','? children
//! branch    := 'if' thunk children ('else' children)?
//! thunk     := rust-closure                   `move`, `|` or `||` is its first token
//! ```
//!
//! Text is a string literal rather than a bare word, because a bare word would make every Rust
//! expression in a view ambiguous. `"Count: "{count}` is a fragment of two children.
//!
//! A node is closed by its own brace, so nothing is written to close one and there is no closing
//! form to get wrong. A head on its own is not a node: `row()` and `row {}` are, and `row` is not.
//!
//! # Control flow
//!
//! `for` and `if` are the only lower-case words that begin a node rather than a call, and each is
//! sugar for a component of the same meaning: `For` and `Show`, which stay writable as calls and
//! are what a view needs in scope to use the keyword.
//!
//! ```
//! # extern crate zgui_view as zgui;
//! # use zgui_view::prelude::*;
//! # use zgui_view_macro::view;
//! # #[derive(Clone)]
//! # struct Todo { id: usize, label: String }
//! # let items: RwSignal<Vec<Todo>> = RwSignal::new(Vec::new());
//! view! {
//!     for item in move || items.get(), key = |item: &Todo| item.id {
//!         {item.label.clone()}
//!     }
//!     if move || items.get().is_empty() {
//!         "Nothing to do yet."
//!     } else {
//!         {move || format!("{} left", items.get().len())}
//!     }
//! }
//! # ;
//! ```
//!
//! The collection and the condition are read again every time what they depend on changes, so each
//! is required to *be* a closure: `for item in items.get()` is a parse error on `items`, and there
//! is no spelling under which a list or a conditional reads a snapshot. What is written is copied
//! into the component unchanged, so a closure written without `move` stays written without one.
//!
//! An `else` with nothing in it is the alternative that renders nothing, and `else if` is not
//! written — a nested `else { if … }` says the same thing and says that each arm is a conditional
//! of its own.
//!
//! # Attributes
//!
//! | Written | Means |
//! |---|---|
//! | `name=value` | a typed attribute of the element, or a prop of the component |
//! | `name` | the same, with a variable of that name as the value |
//! | `class="a b"` | the whole class list |
//! | `class:name=on` | one class, toggled |
//! | `style="…"` | the whole inline style text |
//! | `style:property=value` | one declaration |
//! | `var:--name=value` | one custom property |
//! | `attr:name=value` | an arbitrary attribute, which selectors can see |
//! | `prop:name=value` | an imperative element property, which they cannot |
//! | `state:name=on` | one of the states a view may assert |
//! | `custom_state:name=on` | an author-defined state, matched by `:state(name)` |
//! | `on:event=handler` | a listener |
//! | `a11y:name=value` | one accessibility property |
//! | `node_ref=r` | where to record this element's node once it exists |
//! | `let:name` | names the argument a component passes to its children |
//! | `{..bundle}` | replays a forwarded bundle of attributes here |
//!
//! A toggle written with no value is on: `state:disabled` means `state:disabled=true`.
//!
//! A value is one Rust expression, ending at the `,` or `)` that follows it, so a comparison, a
//! shift and a typed closure parameter are all written unbraced. Braces around a value are
//! optional and are unwrapped: `flag={a > b}` and `flag = a > b` are the same attribute.
//!
//! Whether an attribute is static or dynamic is decided by its **type**, not by how it is
//! written. A literal is written once at build time; a signal or a closure gets exactly one
//! effect. Nothing is annotated to say which.
//!
//! ## Listeners
//!
//! `on:` names are snake_case and each resolves to a constant whose own type carries the payload,
//! so a handler's argument type is inferred and a misspelling is a compile error with a
//! suggestion. Modifiers follow the name: `on:click:once`, `on:wheel:passive`,
//! `on:pointer_down:capture`, and `:prevent` and `:stop`, which suppress the default behaviour and
//! stop the event travelling before the handler runs.
//!
//! A component's callback prop — conventionally `on_select`, `on_open_change` — is an ordinary
//! prop and is **not** a listener: it does not participate in capture and bubble, and it is
//! written `on_select=…`, not `on:select=…`.
//!
//! # What the expansion names
//!
//! A view expands to calls on the view layer and on an element vocabulary, and it reaches both
//! through one crate root — `::zgui::expansion` — so that a crate writing views depends on one
//! crate rather than on each crate an expansion happens to touch, and never on this crate's own
//! dependencies. A crate with no umbrella over it names that root itself:
//!
//! ```
//! extern crate zgui_view as zgui;
//! # fn main() {}
//! ```

#![deny(missing_docs)]
#![forbid(unsafe_code)]

mod component;
mod css;
mod slot;
mod variants;
mod view;

use proc_macro::TokenStream;
use quote::quote;

/// Reports an error where an expression was expected.
///
/// `compile_error!` expands to a statement, and a statement in expression position is a second,
/// confusing error on top of the real one; a block is an expression whatever is inside it.
fn as_expression(error: syn::Error) -> proc_macro2::TokenStream {
    let error = error.into_compile_error();
    quote!({ #error })
}

/// Describes a user interface.
///
/// See the [crate documentation](crate) for the grammar and the attribute forms.
///
/// ```
/// # extern crate zgui_view as zgui;
/// use zgui_view::prelude::*;
/// use zgui_view_macro::{component, view};
///
/// #[component]
/// fn Panel(
///     /// Whether the panel is showing.
///     open: RwSignal<bool>,
///     children: Children,
/// ) -> impl IntoView {
///     view! {
///         Frame(
///             class = "panel",
///             class:open = move || open.get(),
///             a11y:role = Role::Group,
///             on:click:stop = move |_| open.update(|open| *open = !*open)
///         ) {
///             {children.into_view_once()}
///         }
///     }
/// }
///
/// # #[component]
/// # fn Frame(#[prop(attrs)] attrs: Attrs, #[prop(into, optional)] class: Classes,
/// #          children: Children) -> impl IntoView { children.into_view_once() }
/// ```
#[proc_macro]
pub fn view(input: TokenStream) -> TokenStream {
    view::expand(input.into())
        .unwrap_or_else(as_expression)
        .into()
}

/// Turns a function into a component: a props struct, a builder for it, and a scope of its own.
///
/// Every prop is a named argument. A prop with no attribute is required, and leaving it out is a
/// compile error naming the prop rather than a panic at run time.
///
/// | Attribute | Effect |
/// |---|---|
/// | `#[prop(into)]` | the setter accepts anything that converts into the prop's type |
/// | `#[prop(optional)]` | the prop may be left out, and defaults |
/// | `#[prop(default = expr)]` | the prop may be left out, and is `expr` when it is |
/// | `#[prop(name = "…")]` | the prop is written under another name, for reserved words |
/// | `#[prop(attrs)]` | the prop receives what the caller forwarded with `{..attrs}` |
///
/// A prop declared `Option<T>` takes a `T`: `icon=path` rather than `icon=Some(path)`.
///
/// `#[component(slot_aware)]` says the component takes slot children; see [`macro@slot`].
///
/// ```
/// # extern crate zgui_view as zgui;
/// use zgui_view::prelude::*;
/// use zgui_view_macro::{component, view};
///
/// /// A labelled value.
/// #[component]
/// fn Field(
///     /// The label.
///     #[prop(into)]
///     label: String,
///     /// Shown after the label when there is one.
///     #[prop(into, optional)]
///     hint: Option<String>,
///     /// How many columns the field spans.
///     #[prop(default = 1)]
///     span: u8,
/// ) -> impl IntoView {
///     view! { {label}{hint}{span.to_string()} }
/// }
///
/// // `hint` and `span` may be left out; `label` may not.
/// let field = view! { Field(label = "Name") };
/// let spanned = view! { Field(label = "Address", hint = "optional", span = 2) };
/// ```
#[proc_macro_attribute]
pub fn component(attribute: TokenStream, item: TokenStream) -> TokenStream {
    component::expand(attribute.into(), item.into())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Declares a named group of children a component takes beside its ordinary ones.
///
/// A slot is written as a child of the component it belongs to and marked with `slot`. The prop it
/// fills is the slot's own name in snake case, or whatever `slot="…"` says.
///
/// ```
/// # extern crate zgui_view as zgui;
/// use zgui_view::prelude::*;
/// use zgui_view_macro::{component, slot, view};
///
/// /// The heading of a [`Card`].
/// #[slot]
/// pub struct CardHeader {
///     /// What the heading shows.
///     children: Children,
/// }
///
/// /// A card, with an optional heading.
/// #[component(slot_aware)]
/// pub fn Card(
///     /// The heading, when there is one.
///     #[prop(optional)]
///     card_header: Option<CardHeader>,
///     children: Children,
/// ) -> impl IntoView {
///     let heading = card_header.map(|header| header.children.into_view_once());
///     view! { {heading}{children.into_view_once()} }
/// }
///
/// let card = view! {
///     Card {
///         CardHeader(slot) {"Total"}
///         "£12.00"
///     }
/// };
/// ```
#[proc_macro_attribute]
pub fn slot(attribute: TokenStream, item: TokenStream) -> TokenStream {
    slot::expand(attribute.into(), item.into())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Declares a component's visual variants, and what each one is called in CSS.
///
/// The table generates one enumeration per axis with the default the table names, a
/// `classes()` that concatenates them in a **stable** order — so a class list is diffable and a
/// transcript is deterministic — and a `data_attributes()` that reports the same choice as
/// `data-*` attributes, which is what a stylesheet matches on rather than concatenating strings
/// at run time.
///
/// ```
/// # extern crate zgui_view as zgui;
/// use zgui_view_macro::variants;
///
/// variants! {
///     /// Visual variants of a button.
///     pub ButtonVariants {
///         base: "zui-btn",
///         variant: { Default => "zui-btn--default", Outline => "zui-btn--outline" } = Default,
///         size: { Sm => "zui-btn--sm", Md => "" } = Md,
///     }
/// }
///
/// let outline = ButtonVariants {
///     variant: ButtonVariant::Outline,
///     ..ButtonVariants::default()
/// };
/// assert_eq!(outline.class_list(), "zui-btn zui-btn--outline");
/// assert_eq!(
///     outline.data_attributes(),
///     [("data-variant", "outline"), ("data-size", "md")]
/// );
/// ```
#[proc_macro]
pub fn variants(input: TokenStream) -> TokenStream {
    variants::expand(input.into())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Checks a block of CSS at compile time and hands back the text.
///
/// The check happens where the CSS is written, so an unterminated string, an unbalanced block or
/// a declaration with no value is reported against the source rather than warned about at run
/// time, once, when the sheet is loaded.
///
/// ```
/// use zgui_view_macro::css;
///
/// const SHEET: &str = css!(".card { padding: 1rem; border-radius: 8px; }");
/// assert!(SHEET.contains("border-radius"));
/// ```
///
/// ```compile_fail
/// use zgui_view_macro::css;
/// const BROKEN: &str = css!(".card { padding: 1rem; ");
/// ```
#[proc_macro]
pub fn css(input: TokenStream) -> TokenStream {
    css::expand_css(input.into())
        .unwrap_or_else(as_expression)
        .into()
}

/// Declares a component's own stylesheet, scoped to a class nothing else can collide with.
///
/// `:scope` is rewritten to the generated class at compile time, so the rules are ordinary CSS
/// and need no run-time rewriting. The class is derived from the name and the text, so it is
/// stable across builds and changes when the sheet does.
///
/// ```
/// use zgui_view_macro::style;
///
/// style! { pub Button =>
///     ":scope { display: inline-flex; align-items: center; }"
///     ":scope[data-disabled] { opacity: .5; }"
/// }
///
/// // The class a component puts on its own root …
/// assert!(Button::CLASS.starts_with("zs-"));
/// // … and the text, with `:scope` already resolved to it.
/// assert!(Button::CSS.contains(Button::CLASS));
/// assert!(!Button::CSS.contains(":scope"));
/// ```
#[proc_macro]
pub fn style(input: TokenStream) -> TokenStream {
    css::expand_style(input.into())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
