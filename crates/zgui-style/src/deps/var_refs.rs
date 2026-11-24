//! Which custom properties an element's declarations read.
//!
//! A change to a custom property reaches an element through `var()`, and only through `var()`:
//! an element none of whose declarations mention the property computes the same values whatever
//! it holds. The engine cannot say which declarations mention it — the parsed references live in
//! a private field — so the declarations are serialised and the text scanned, once per
//! declaration block, and the answer is cached by the rule node that names the block chain.

use style::properties::declaration_block::AppendableValue;
use style::properties::{
    CustomDeclarationValue, PropertyDeclaration, PropertyDeclarationBlock, PropertyDeclarationId,
};
use style::rule_tree::StrongRuleNode;
use style::shared_lock::StylesheetGuards;
use style::stylesheets::keyframes_rule::{KeyframesAnimation, KeyframesStepValue};

/// What one element's whole rule chain says about custom properties.
#[derive(Debug, Default)]
pub(crate) struct RuleRefs {
    /// The custom properties read through `var()`, without their `--` prefix.
    pub(crate) reads: Vec<String>,
    /// Whether the chain declares a custom property of its own.
    ///
    /// A declarer builds its map from the parent's and its own declarations, so a moved parent
    /// map is a moved map of its own however its declarations read; it cascades again.
    pub(crate) declares: bool,
    /// The custom properties the chain declares, without their `--` prefix.
    ///
    /// With the names its parent's map changed by, these are every name a declarer's own map can
    /// change by when nothing else about it moved.
    pub(crate) declared: Vec<String>,
    /// The Bloom set of `reads`, or every name for a declarer.
    ///
    /// What the document's readers column files for the element, so that a change tests a whole
    /// subtree with one `and`. See [`zgui_dom::side::readers`].
    pub(crate) bloom: u64,
}

impl RuleRefs {
    /// Whether any of `names` is read.
    pub(crate) fn reads_any(&self, names: &[String]) -> bool {
        self.reads
            .iter()
            .any(|read| names.iter().any(|name| name == read))
    }
}

/// Everything the chain of rules ending at `rules` reads and declares.
pub(crate) fn refs_of(rules: &StrongRuleNode, guards: &StylesheetGuards<'_>) -> RuleRefs {
    let mut refs = RuleRefs::default();
    let mut text = String::new();
    for node in rules.self_and_ancestors() {
        let Some(source) = node.style_source() else {
            continue;
        };
        let guard = guards.for_origin(node.cascade_level().origin().origin());
        scan_block(source.read(guard), &mut refs, &mut text);
    }
    refs.finish();
    refs
}

/// Everything the `@keyframes` rule `animation` reads through `var()`.
///
/// A keyframe's custom property declarations are ignored by the engine, so the answer only reads.
pub(crate) fn keyframe_refs_of(
    animation: &KeyframesAnimation,
    guards: &StylesheetGuards<'_>,
) -> RuleRefs {
    let mut refs = RuleRefs::default();
    let mut text = String::new();
    for step in animation
        .steps
        .iter()
        .chain(&animation.steps_with_range_name)
    {
        if let KeyframesStepValue::Declarations { block } = &step.value {
            scan_block(block.read_with(guards.author), &mut refs, &mut text);
        }
    }
    refs.declares = false;
    refs.declared.clear();
    refs.finish();
    refs
}

/// Adds what one declaration block reads and declares to `refs`.
///
/// `text` is scratch space for the serialised declarations.
fn scan_block(block: &PropertyDeclarationBlock, refs: &mut RuleRefs, text: &mut String) {
    let mut last_unparsed: usize = 0;
    for declaration in block.declarations() {
        match declaration {
            PropertyDeclaration::Custom(custom) => {
                refs.declares = true;
                refs.declared.push(custom.name.to_string());
                if let CustomDeclarationValue::Unparsed(value) = &custom.value {
                    scan_var_names(&value.css, &mut refs.reads);
                }
            }
            PropertyDeclaration::WithVariables(variable) => {
                let ptr = variable.value.heap_ptr() as usize;
                if ptr == last_unparsed {
                    continue;
                }
                last_unparsed = ptr;
                text.clear();
                // A longhand expanded from a shorthand that contained `var()` is a
                // pending-substitution value whose own CSSOM serialisation is empty; its raw
                // shorthand text is recovered by asking the shorthand it came from about this
                // single declaration.
                if declaration.to_css(text).is_ok() && !text.is_empty() {
                    scan_var_names(text, &mut refs.reads);
                } else if let PropertyDeclarationId::Longhand(longhand) = declaration.id() {
                    for shorthand in longhand.shorthands() {
                        if let Some(AppendableValue::Css(css)) = shorthand
                            .get_shorthand_appendable_value(core::slice::from_ref(&declaration))
                        {
                            scan_var_names(css, &mut refs.reads);
                            break;
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

impl RuleRefs {
    /// Sorts and deduplicates the names, and files the Bloom set.
    fn finish(&mut self) {
        self.reads.sort_unstable();
        self.reads.dedup();
        self.declared.sort_unstable();
        self.declared.dedup();
        self.bloom = if self.declares {
            zgui_dom::side::readers::ALL
        } else {
            self.reads.iter().fold(0, |bits, name| {
                bits | zgui_dom::side::readers::name_bits(name)
            })
        };
    }
}

/// Finds the next ASCII case-insensitive `"var("` call in `text`.
fn find_var_open(text: &str) -> Option<usize> {
    text.as_bytes()
        .windows(4)
        .position(|w| w.eq_ignore_ascii_case(b"var("))
}

/// Appends every custom property name `var(--name` in `text` names, without the prefix.
pub(crate) fn scan_var_names(text: &str, out: &mut Vec<String>) {
    let mut rest = text;
    while let Some(at) = find_var_open(rest) {
        rest = &rest[at + "var(".len()..];
        let trimmed = rest.trim_start();
        let Some(named) = trimmed.strip_prefix("--") else {
            continue;
        };
        let end = named
            .find(|c: char| !(c.is_alphanumeric() || c == '-' || c == '_'))
            .unwrap_or(named.len());
        if end > 0 {
            out.push(named[..end].to_owned());
        }
        rest = &named[end..];
    }
}

#[cfg(test)]
mod tests {
    use super::scan_var_names;

    fn names(text: &str) -> Vec<String> {
        let mut out = Vec::new();
        scan_var_names(text, &mut out);
        out
    }

    #[test]
    fn every_reference_is_found_including_nested_and_fallback_ones() {
        assert_eq!(names("var(--a)"), ["a"]);
        assert_eq!(names("var( --a-b , var(--c_d))"), ["a-b", "c_d"]);
        assert_eq!(
            names("1px solid var(--zgui-border) inset var(--x)"),
            ["zgui-border", "x"]
        );
        assert_eq!(names("1px solid VAR(--up)"), ["up"]);
        assert!(names("rgb(1, 2, 3)").is_empty());
        assert!(names("var(a)").is_empty());
    }
}
