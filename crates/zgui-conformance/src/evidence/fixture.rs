//! The document every probe is run against.
//!
//! One document, not one per property, because the question is *"does this property change
//! anything at all"* and a per-property fixture would be a per-property opportunity to write one
//! that cannot answer. It is built so that the probed element is, at the same time, a flex item, a
//! grid item, an in-flow block, a block container holding text, the parent of an inline, the parent
//! of a nested block and the parent of replaced content — which is the union of the positions the
//! layout engine treats differently.

use crate::zdoc::Zdoc;

/// The style sheet the probed document is laid out under.
///
/// The probed elements are deliberately positioned (so an inset has something to move), sized
/// explicitly in one place and left automatic in another (so both a definite and an intrinsic
/// size are exercised), and one of them is a list item (so a marker exists to be styled).
///
/// Three of the containers exist for one property each, because a property that only decides what
/// happens to a *leftover* has nothing to decide in a container that has none: `.flexwrap` is taller
/// than its lines so that `align-content` has free space to distribute, `.squeeze` is narrower than
/// its items so that `flex-shrink` has an overflow to take back, and `.gridcols` flows along the
/// inline axis over one explicit row, so that its second and third items land in implicit columns —
/// two of them, because the width of the last track moves nothing and only the width of the one
/// before it can be seen.
const SHEET: &str = "\
root { display: block; width: 400px; height: 300px }
.flexbox { display: flex; width: 300px; height: 120px }
.gridbox { display: grid; grid-template-columns: 80px 80px; grid-template-rows: 40px; width: 300px }
.blockbox { display: block; width: 300px }
.flexwrap { display: flex; flex-wrap: wrap; align-content: flex-start; width: 100px; height: 150px }
.squeeze { display: flex; width: 60px; height: 40px }
.gridcols { display: grid; grid-auto-flow: column; grid-template-columns: 40px; grid-template-rows: 40px; width: 300px }
.cell { width: 60px; height: 30px; position: relative; padding: 3px; outline: 1px solid }
.auto { display: block }
.item { display: list-item }
.narrow { display: block; width: 40px }
.floated { float: left; width: 24px; height: 12px }
.clipped { display: block; overflow: hidden; width: 30px; height: 20px }
.turned { display: block; width: 20px; height: 20px; transform: scale(2) }
.inner { display: block; width: 20px; height: 10px }
";

/// The element tree, written once so that the probed and unprobed runs cannot drift apart.
///
/// The probe class sits on the containers as well as on their children, because roughly half the
/// layout vocabulary is a property a *container* reads about its children — a fixture that probed
/// only the children would report `flex-direction` and `grid-template-columns` as having no effect.
///
/// The text holds a run of two spaces and a tab, neither of which a document would ordinarily
/// bother with. They are what `white-space-collapse` and `tab-size` act on: text with one space
/// between each word collapses to itself, so a fixture written that way reports both properties as
/// having no effect however faithfully they are honoured.
const TREE: &str = "\
root
  div.flexbox.probe
    div.cell.probe \"ab  cd\tef\"
      span \"x\"
      div.inner
      img [40x30]
    div.cell
  div.flexwrap.probe
    div.cell.probe
    div.cell
    div.cell
  div.squeeze.probe
    div.cell.probe
    div.cell
  div.gridbox.probe
    div.cell.probe
    div.cell
    div.cell
  div.gridcols.probe
    div.cell.probe
    div.cell
    div.cell
  div.blockbox.probe
    div.cell.probe \"ab  cd\tef\"
    div.auto.probe \"gh ij kl mn op qr\"
      img [40x30]
    div.item.probe \"st\"
    div.floated
    div.narrow.probe \"abcdefghijklm nop\"
    div.clipped.probe
      div.inner
    div.turned.probe
";

/// The document with no probe declaration in it, which every comparison is against.
pub fn baseline() -> Zdoc {
    document("")
}

/// The same document with `declaration` applied to the probed elements.
///
/// The declaration is applied to a generated-content pseudo-element as well, which is the only way
/// a property that exists to make one appear can be seen to do anything: without it, nothing in the
/// document has a `::before` for `content` to fill.
pub fn probed(declaration: &str) -> Zdoc {
    document(&format!(
        ".probe {{ {declaration} }}\n.probe::before {{ {declaration} }}\n"
    ))
}

/// Assembles the document with `extra` appended to the sheet.
fn document(extra: &str) -> Zdoc {
    Zdoc::parse(&format!(
        "@viewport 400 300\n@css\n{SHEET}{extra}@tree\n{TREE}"
    ))
    .expect("the probe fixture is well formed")
}

#[cfg(test)]
mod tests {
    use super::{baseline, probed};
    use crate::fragment;
    use crate::zdoc::build::lay_out;

    /// The fixture lays out, and lays out the same way twice.
    ///
    /// The control for every probe above it: a comparison whose two sides differed by themselves
    /// would report every property as having an effect, including the ones that have none.
    #[test]
    fn the_fixture_is_deterministic() {
        let first = fragment::full(&lay_out(&baseline()).store);
        let second = fragment::full(&lay_out(&baseline()).store);
        assert_eq!(first, second);
        assert!(first.lines().count() > 10, "{first}");
    }

    /// A declaration that changes the layout changes the rendering.
    ///
    /// The other control: a rendering that could not change would hold every probe green while
    /// measuring nothing.
    #[test]
    fn the_fixture_notices_a_change() {
        let before = fragment::full(&lay_out(&baseline()).store);
        let after = fragment::full(&lay_out(&probed("width: 17px")).store);
        assert_ne!(before, after);
    }
}
