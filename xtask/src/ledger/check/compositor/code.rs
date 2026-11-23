//! Finding compositor implementations named in source text.

use crate::ledger::check::compositor::CONTRACT;

/// The concrete compositor, renderer and presenter types of the implementation crates.
const CONCRETE: &[&str] = &[
    "CompositorImpl",
    "WgpuRenderer",
    "CaptureRenderer",
    "SurfacePresenter",
    "Offscreen",
    "Supplied",
];

/// The calls that turn a trait object into a concrete type.
const DOWNCASTS: &[&str] = &[
    "downcast_ref",
    "downcast_mut",
    "downcast::<",
    ".downcast(",
    "is::<",
];

/// Returns each line of `text` that names one of the `implementations` or downcasts to a concrete
/// type, by line number from one, with what it names.
///
/// An implementation is named when a path starts with its crate name. Comment lines are skipped.
pub(crate) fn findings(text: &str, implementations: &[&str]) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim_start().starts_with("//") {
            continue;
        }
        if let Some(path) = crate_path(line, implementations) {
            out.push((index + 1, format!("names the implementation `{path}`")));
        } else if let Some(target) = downcast_target(line) {
            out.push((index + 1, format!("downcasts to the concrete `{target}`")));
        }
    }
    out
}

/// Returns the implementation crate whose path `line` names, if any.
fn crate_path(line: &str, implementations: &[&str]) -> Option<String> {
    implementations
        .iter()
        .map(|name| name.replace('-', "_"))
        .find(|path| path_root_at(line, path).is_some())
}

/// Returns the concrete type a downcast on `line` targets, if any.
fn downcast_target(line: &str) -> Option<&'static str> {
    if !DOWNCASTS.iter().any(|call| line.contains(call)) {
        return None;
    }
    CONCRETE
        .iter()
        .copied()
        .find(|name| word_at(line, name).is_some())
}

/// Returns where `root` first stands in `line` outside a module of a contract crate.
fn path_root_at(line: &str, root: &str) -> Option<usize> {
    let identifier = |c: char| c.is_alphanumeric() || c == '_';
    let in_contract = |before: &str| {
        CONTRACT.iter().any(|name| {
            before
                .strip_suffix(name.replace('-', "_").as_str())
                .is_some_and(|rest| !rest.ends_with(identifier))
        })
    };
    // `compositor_types::compositor_color` is a module of the contract crate.
    words_at(line, root).find(|&at| !line[..at].strip_suffix("::").is_some_and(in_contract))
}

/// Returns where `word` first stands in `line` as a whole identifier.
fn word_at(line: &str, word: &str) -> Option<usize> {
    words_at(line, word).next()
}

/// Returns each place `word` stands in `line` as a whole identifier.
fn words_at<'a>(line: &'a str, word: &'a str) -> impl Iterator<Item = usize> + 'a {
    let identifier = |c: char| c.is_alphanumeric() || c == '_';
    line.match_indices(word)
        .map(|(at, _)| at)
        .filter(move |&at| {
            let before = line[..at].chars().next_back();
            let after = line[at + word.len()..].chars().next();
            !before.is_some_and(identifier) && !after.is_some_and(identifier)
        })
}

#[cfg(test)]
mod tests {
    use super::findings as findings_in;

    /// The findings in `text`, against a few implementations.
    fn findings(text: &str) -> Vec<(usize, String)> {
        findings_in(
            text,
            &[
                "compositor-core",
                "compositor-text",
                "compositor-vector",
                "compositor-vector-coverage",
                "compositor-color",
            ],
        )
    }

    #[test]
    fn a_path_into_an_implementation_is_found_and_the_interface_is_not() {
        assert_eq!(findings("use compositor_core::CompositorImpl;").len(), 1);
        assert_eq!(
            findings("let raster = compositor_text::SwashRaster::new();").len(),
            1
        );
        assert!(findings("use compositor_api::Compositor;").is_empty());
        assert!(findings("use compositor_types::Transaction;").is_empty());
    }

    #[test]
    fn a_module_of_the_contract_is_not_the_crate_it_shares_a_name_with() {
        assert!(findings("use compositor_types::compositor_color::Color;").is_empty());
        assert_eq!(findings("use ::compositor_color::Color;").len(), 1);
        assert_eq!(
            findings("use zgui_testkit::compositor_core::CompositorImpl;").len(),
            1
        );
    }

    #[test]
    fn a_crate_name_inside_a_longer_one_is_the_longer_one() {
        let found = findings("use compositor_vector_coverage::Raster;");
        assert_eq!(found.len(), 1);
        assert!(found[0].1.contains("compositor_vector_coverage"));
    }

    #[test]
    fn a_downcast_to_a_concrete_renderer_is_found() {
        assert_eq!(
            findings("renderer.as_any_mut().and_then(|any| any.downcast_mut::<WgpuRenderer>())")
                .len(),
            1
        );
        assert!(findings("any.downcast_mut::<V::State>()").is_empty());
    }

    #[test]
    fn a_comment_is_not_code() {
        assert!(findings("// built by compositor_core::CompositorImpl").is_empty());
    }
}
