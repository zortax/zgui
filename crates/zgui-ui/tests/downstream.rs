//! The claim this crate is built on: it is an ordinary consumer of the public API.
//!
//! Not a comment, because a comment cannot fail. A component that reached past the umbrella crate
//! would be a component nobody outside this workspace could have written, and every hole it papered
//! over would stay open for the application author who meets it next.

use std::path::Path;

/// This crate's manifest.
fn manifest() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    std::fs::read_to_string(path).expect("this crate has a manifest")
}

/// Every `.rs` file under this crate's `src`.
fn sources() -> Vec<(String, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(directory) = stack.pop() {
        for entry in std::fs::read_dir(&directory).expect("the source directory is readable") {
            let path = entry.expect("a directory entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|kind| kind == "rs") {
                let text = std::fs::read_to_string(&path).expect("a source file is readable");
                out.push((path.display().to_string(), text));
            }
        }
    }
    assert!(!out.is_empty(), "no sources were found to check");
    out
}

#[test]
fn the_library_depends_on_the_umbrella_and_on_its_own_siblings_and_on_nothing_else() {
    let manifest = manifest();
    let dependencies: Vec<&str> = manifest
        .lines()
        .skip_while(|line| line.trim() != "[dependencies]")
        .skip(1)
        .take_while(|line| !line.trim_start().starts_with('['))
        .filter_map(|line| line.split('=').next())
        .map(str::trim)
        .filter(|name| !name.is_empty() && !name.starts_with('#'))
        .collect();

    let allowed = [
        "zgui",
        "zgui-ui-icons",
        "zgui-ui-primitives",
        "zgui-ui-tokens",
    ];
    for name in &dependencies {
        assert!(
            allowed.contains(name),
            "`{name}` is a dependency an application would not have; whatever it provides belongs \
             in the public API instead"
        );
    }
    assert!(dependencies.contains(&"zgui"), "the umbrella is the edge");
}

#[test]
fn no_component_names_a_crate_below_the_public_api() {
    // The manifest is one half of the claim and this is the other: a path dependency reachable
    // through the umbrella's own re-exports is still a crate an application cannot name, and
    // `zgui::…` is how an application reaches every one of them.
    let hidden = [
        "zgui_dom",
        "zgui_style",
        "zgui_layout",
        "zgui_paint",
        "zgui_input",
        "zgui_scroll",
        "zgui_a11y",
        "zgui_anim",
        "zgui_edit",
        "zgui_runtime",
        "zgui_view_dom",
        "zgui_css",
        "zgui_interned",
        "zgui_scene",
        "zgui_render",
    ];
    for (path, text) in sources() {
        for name in hidden {
            assert!(
                !text.contains(&format!("{name}::")),
                "{path} names `{name}`, which an application author cannot"
            );
        }
    }
}

#[test]
fn no_component_keeps_its_own_copy_of_an_interaction_state() {
    // The defect this catches is a component that tracks `:hover` or `:focus` in a signal. It
    // works, right up to the frame the pointer leaves without a `pointer_leave` arriving — and
    // then there are two answers to one question and the wrong one is the one that is drawn.
    let forbidden = [
        "hovered",
        "is_hover",
        "focused",
        "is_focus",
        "pressed_state",
        "active_state",
    ];
    for (path, text) in sources() {
        for name in forbidden {
            assert!(
                !text.contains(&format!("{name} = RwSignal")),
                "{path} keeps `{name}` as state; the engine already knows, and `:hover`, \
                 `:focus-visible` and `:active` are how a sheet reads its answer"
            );
        }
    }
}

#[test]
fn every_component_that_renders_an_element_forwards_what_its_caller_wrote() {
    // A component with no `#[prop(attrs)]` is one whose caller cannot add a test identifier, a
    // class, or an accessibility label — and the only way to find that out is to try to use it.
    //
    // A component that renders *no element of its own* is exempt, and that is not a loophole: a
    // root that only publishes a context and hands its children straight back has nowhere to put
    // an attribute, and a bundle it accepted and dropped would be worse than not taking one.
    for (path, text) in sources() {
        for chunk in components_of(&text) {
            if declares_a_bundle(chunk) || !renders_an_element(chunk) {
                continue;
            }
            panic!(
                "{path} declares a component that renders an element and forwards no bundle:\n{}",
                chunk.lines().take(4).collect::<Vec<_>>().join("\n")
            );
        }
    }
}

/// The source of each component in one file, split at its attribute.
fn components_of(text: &str) -> Vec<&str> {
    let mut chunks = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find("\n#[component]\n") {
        rest = &rest[at + 1..];
        let end = rest[1..]
            .find("\n#[component]\n")
            .map_or(rest.len(), |next| next + 1);
        chunks.push(&rest[..end]);
    }
    chunks
}

/// Whether one component takes a forwarded attribute bundle.
fn declares_a_bundle(chunk: &str) -> bool {
    chunk
        .lines()
        .any(|line| line.trim_start() == "#[prop(attrs)]")
}

/// Whether one component builds an element of its own.
///
/// An element is written as a lower-case tag and a component call as an upper-case one, so the
/// distinction is legible without parsing the macro. Doc comments are skipped: a worked example
/// showing an element is not the component rendering one.
fn renders_an_element(chunk: &str) -> bool {
    chunk.lines().any(|line| {
        let trimmed = line.trim_start();
        !trimmed.starts_with("//")
            && trimmed.starts_with('<')
            && trimmed[1..].starts_with(|first: char| first.is_ascii_lowercase())
    })
}
