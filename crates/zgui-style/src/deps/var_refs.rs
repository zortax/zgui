//! Which custom properties an element's declarations read.
//!
//! A change to a custom property reaches an element through `var()`, and only through `var()`:
//! an element none of whose declarations mention the property computes the same values whatever
//! it holds. The engine cannot say which declarations mention it — the parsed references live in
//! a private field — so the declarations are serialised and the text scanned, once per
//! declaration block, and the answer is cached by the rule node that names the block chain.

use style::properties::{CustomDeclarationValue, PropertyDeclaration};
use style::rule_tree::StrongRuleNode;
use style::shared_lock::StylesheetGuards;

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
        let block = source.read(guard);
        for declaration in block.declarations() {
            match declaration {
                PropertyDeclaration::Custom(custom) => {
                    refs.declares = true;
                    refs.declared.push(custom.name.to_string());
                    if let CustomDeclarationValue::Unparsed(value) = &custom.value {
                        scan_var_names(&value.css, &mut refs.reads);
                    }
                }
                PropertyDeclaration::WithVariables(_) => {
                    text.clear();
                    if declaration.to_css(&mut text).is_ok() {
                        scan_var_names(&text, &mut refs.reads);
                    }
                }
                _ => {}
            }
        }
    }
    refs.reads.sort_unstable();
    refs.reads.dedup();
    refs.declared.sort_unstable();
    refs.declared.dedup();
    refs.bloom = if refs.declares {
        zgui_dom::side::readers::ALL
    } else {
        refs.reads.iter().fold(0, |bits, name| {
            bits | zgui_dom::side::readers::name_bits(name)
        })
    };
    refs
}

/// Appends every custom property name `var(--name` in `text` names, without the prefix.
pub(crate) fn scan_var_names(text: &str, out: &mut Vec<String>) {
    let mut rest = text;
    while let Some(at) = rest.find("var(") {
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
        assert!(names("rgb(1, 2, 3)").is_empty());
        assert!(names("var(a)").is_empty());
    }
}
