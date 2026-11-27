//! Finding compositor implementations named in source text.

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
        .find(|path| word_at(line, path).is_some())
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
        findings_in(text, &["iceblink-core", "iceblink-text", "iceblink-wgpu"])
    }

    #[test]
    fn a_path_into_an_implementation_is_found_and_the_interface_is_not() {
        assert_eq!(findings("use iceblink_core::CompositorImpl;").len(), 1);
        assert_eq!(
            findings("let raster = iceblink_text::SwashRaster::new();").len(),
            1
        );
        assert!(findings("use iceblink::Compositor;").is_empty());
        assert!(findings("use iceblink::geom::Rect;").is_empty());
    }

    #[test]
    fn an_implementation_reached_through_another_path_is_found() {
        assert_eq!(findings("use ::iceblink_core::CompositorImpl;").len(), 1);
        assert_eq!(
            findings("use zgui_testkit::iceblink_core::CompositorImpl;").len(),
            1
        );
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
        assert!(findings("// built by iceblink_core::CompositorImpl").is_empty());
    }
}
