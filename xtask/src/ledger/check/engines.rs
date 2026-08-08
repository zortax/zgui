//! The engine-naming ledger.
//!
//! A crate names an external engine, or it does not. Every engine is reachable from a bounded,
//! enumerable set of crates, and this table is the architecture: if it drifts, the architecture
//! has drifted.

use std::collections::BTreeMap;

use crate::ledger::report::Report;
use crate::ledger::tree::Tree;
use crate::ledger::tree::manifest::Dependency;
use crate::ledger::tree::member::Member;

/// Crates that may never be named, in any manifest, in any section.
///
/// The renderer is our own. The reference implementation is material to read, never to link.
const BANNED_PREFIXES: [&str; 1] = ["gpui"];

/// Which crates may name which engine.
const LEDGER: &[(&str, &[&str])] = &[
    ("stylo", &["zgui-css", "zgui-dom", "zgui-style"]),
    ("stylo_dom", &["zgui-css", "zgui-dom", "zgui-style"]),
    ("stylo_atoms", &["zgui-css", "zgui-dom", "zgui-style"]),
    (
        "stylo_static_prefs",
        &["zgui-css", "zgui-dom", "zgui-style"],
    ),
    ("selectors", &["zgui-css", "zgui-dom", "zgui-style"]),
    ("servo_arc", &["zgui-css", "zgui-dom", "zgui-style"]),
    ("web_atoms", &["zgui-css", "zgui-dom", "zgui-style"]),
    ("cssparser", &["zgui-css", "zgui-dom", "zgui-style"]),
    // The style engine's length unit and the geometry its container-query hook answers in. Both
    // appear in return position of a required trait method, so the crates that implement that
    // trait surface have to name them — and no other crate may, for exactly the reason the engine
    // itself may not: the firewall is worth nothing if two of its three names go unpoliced.
    ("app_units", &["zgui-css", "zgui-dom", "zgui-style"]),
    ("euclid", &["zgui-css", "zgui-dom", "zgui-style"]),
    ("taffy", &["zgui-layout"]),
    // A parser and nothing else: it reads an SVG document into a tree of paths, paints and clips
    // and draws none of it. That is why only the crate that owns the parse boundary may name it —
    // a document is mapped onto this framework's own vector model there, and both path rasterisers
    // draw the result without either of them knowing that SVG exists. A reader that produced one
    // rasteriser's scene type would have bound every asset in an application to that rasteriser.
    ("usvg", &["zgui-svg"]),
    ("parley", &["zgui-text-parley"]),
    ("fontique", &["zgui-text-parley"]),
    ("harfrust", &["zgui-text-parley"]),
    ("skrifa", &["zgui-text-parley"]),
    ("swash", &["zgui-text-parley"]),
    // Coverage, not colour: it turns an outline into an alpha mask and knows nothing about what is
    // drawn with it. Two crates rasterise through it and both put the result in the same monochrome
    // atlas — the text stack for glyph outlines, the paint stack for the small solid shapes an icon
    // is made of. Sharing the atlas is what makes recolouring an icon cost a sprite instance rather
    // than a second rasterisation, and it only works while both name the one rasteriser.
    ("zeno", &["zgui-text-parley", "zgui-paint"]),
    ("vello", &["zgui-render-vector-vello"]),
    (
        "wgpu",
        &[
            "zgui-render-wgpu",
            "zgui-render-vector-vello",
            "zgui-render-vector-coverage",
            "zgui-platform-winit",
        ],
    ),
    ("winit", &["zgui-platform-winit"]),
    ("accesskit_winit", &["zgui-platform-winit"]),
    ("arboard", &["zgui-platform-winit"]),
    ("smithay-clipboard", &["zgui-platform-winit"]),
    ("zbus", &["zgui-platform-winit", "zgui-platform-wayland"]),
    // The compositor, spoken to directly. Every one of these names a Wayland protocol or the loop
    // it is read on, and the whole point of a second platform backend is that none of them is
    // reachable from anywhere else: a crate above the seam that named one would be a crate that
    // stops compiling on macOS.
    ("smithay-client-toolkit", &["zgui-platform-wayland"]),
    ("wayland-client", &["zgui-platform-wayland"]),
    ("wayland-backend", &["zgui-platform-wayland"]),
    ("wayland-protocols", &["zgui-platform-wayland"]),
    ("wayland-protocols-wlr", &["zgui-platform-wayland"]),
    ("calloop", &["zgui-platform-wayland"]),
    ("calloop-wayland-source", &["zgui-platform-wayland"]),
    ("accesskit_unix", &["zgui-platform-wayland"]),
    ("reactive_graph", &["zgui-reactive"]),
    ("reactive_stores", &["zgui-reactive"]),
    ("any_spawner", &["zgui-reactive"]),
    ("send_wrapper", &["zgui-reactive"]),
    // A whole async runtime, kept behind one crate that an application opts into. The reason to
    // enforce it here rather than trust it is that the cost is invisible at the call site: a
    // second crate naming tokio "just for a channel" would put a multi-threaded runtime into the
    // dependency graph of every program that links the framework, to buy something `futures` and
    // `zgui-reactive`'s own executor already do.
    ("tokio", &["zgui-tokio"]),
    // The kernel's display interface, reached with no libc. Confined to one crate for the same
    // reason a windowing library is: what it costs to replace is what it costs to find, and a
    // second crate issuing ioctls of its own is a second place a device can be left in a state
    // nothing puts back.
    //
    // `zgui-platform-drm` is here for one descriptor and nothing else: the wake channel the frame
    // loop parks on beside the device is an eventfd, and there is no other safe way to hold one.
    // It issues no ioctl — every one of those stays in `zgui-drm`.
    //
    // The Wayland backend is on this row for one call of its own: the monotonic clock the
    // compositor stamps its presentation feedback against, which a reported presentation is
    // useless without. A row is read by its first match, so the uses share one.
    ("rustix", &["zgui-drm", "zgui-platform-drm", "zgui-platform-wayland"]),
    ("bindgen", &["zgui-drm"]),
    (
        "accesskit",
        &[
            "zgui-vocab",
            "zgui-platform",
            "zgui-a11y",
            "zgui-text",
            "zgui-text-parley",
            "zgui-dom",
            // Every backend of the platform contract names the tree type the contract's own
            // method takes, whatever it does with one.
            "zgui-platform-headless",
            "zgui-platform-winit",
            "zgui-platform-wayland",
            "zgui-platform-drm",
        ],
    ),
    // `zgui-elements` is on the kurbo row because `<vector>`'s outlines are Béziers and there is
    // no lower type for them: the icon set, the chart marks and the rasteriser all name the same
    // one, so a path crosses from a view to the raster with no conversion anywhere. kurbo is pure
    // data — no engine, no device, no threads — so naming it costs the view layer nothing it is
    // meant to be free of.
    (
        "kurbo",
        &[
            "zgui-elements",
            "zgui-scene",
            "zgui-paint",
            // A vector document's outlines are Béziers and its clips are Béziers, and they cross
            // from the parse boundary to the rasteriser as the same type with no conversion.
            "zgui-svg",
            // A glyph's curves are Béziers too: the contract a font engine returns outlines
            // through names the same type the rasteriser fills, so text that leaves the atlas
            // crosses from the engine to the path renderer with no conversion.
            "zgui-text",
            "zgui-text-parley",
            "zgui-render-wgpu",
            "zgui-render-vector-vello",
            "zgui-render-vector-coverage",
            "zgui-ui-icons",
            "zgui-ui-primitives",
            // A canvas scene's shapes are Béziers in the same vocabulary a document resolves to,
            // which is what lets a canvas ride the whole vector pipeline unchanged.
            "zgui-canvas",
        ],
    ),
    (
        "peniko",
        &[
            "zgui-scene",
            "zgui-paint",
            "zgui-render-wgpu",
            "zgui-render-vector-vello",
            "zgui-render-vector-coverage",
            "zgui-svg",
            "zgui-ui-icons",
            "zgui-ui-primitives",
            // Fill rules on a canvas shape are the same fill rules a document's shapes carry.
            "zgui-canvas",
        ],
    ),
    ("etagere", &["zgui-atlas"]),
    (
        "bytemuck",
        &[
            "zgui-geom",
            "zgui-scene",
            "zgui-render-wgpu",
            "zgui-render-vector-vello",
            "zgui-render-vector-coverage",
        ],
    ),
    ("syn", &["zgui-view-macro"]),
    ("quote", &["zgui-view-macro"]),
    ("proc-macro2", &["zgui-view-macro"]),
];

/// Members outside the ledger, and why.
///
/// `probe` exists precisely to build every engine at once; the spikes are running programs that
/// retire into the crate that will own the engine, and each carries the phase that deletes it.
const EXEMPT: [&str; 2] = ["probe", "xtask"];

/// Runs the check.
pub(crate) fn check(tree: &Tree) -> Report {
    let mut report = Report::clean();
    let workspace = tree.manifest.workspace_dependencies();

    for banned in banned_in(&workspace) {
        report.violation(tree.manifest.rel_path.clone(), banned);
    }

    let aliases = aliases(&workspace);
    for member in &tree.members {
        let dependencies: Vec<Dependency> = member
            .manifest
            .dependencies()
            .into_iter()
            .map(|dependency| resolve(dependency, &aliases))
            .collect();
        for banned in banned_in(&dependencies) {
            report.violation(member.manifest.rel_path.clone(), banned);
        }
        if is_exempt(member) {
            continue;
        }
        for dependency in dependencies {
            let Some((_, permitted)) = LEDGER.iter().find(|(engine, _)| *engine == dependency.name)
            else {
                continue;
            };
            if !permitted.contains(&member.name.as_str()) {
                report.violation(
                    member.manifest.rel_path.clone(),
                    format!(
                        "`{}` names `{}`, which only {} may name",
                        member.name,
                        dependency.name,
                        permitted.join(", ")
                    ),
                );
            }
        }
    }
    report
}

/// The `[workspace.dependencies]` keys that stand for a differently named package.
///
/// A member writes `key.workspace = true`, so without this map an engine renamed once in the
/// root manifest would be invisible to the table above for the rest of the tree's life.
fn aliases(workspace: &[Dependency]) -> BTreeMap<&str, &str> {
    workspace
        .iter()
        .filter(|dependency| dependency.key != dependency.name)
        .map(|dependency| (dependency.key.as_str(), dependency.name.as_str()))
        .collect()
}

/// Replaces an inherited entry's key with the package it actually stands for.
fn resolve(mut dependency: Dependency, aliases: &BTreeMap<&str, &str>) -> Dependency {
    if dependency.inherited
        && let Some(real) = aliases.get(dependency.key.as_str())
    {
        (*real).clone_into(&mut dependency.name);
    }
    dependency
}

/// Whether a member sits outside the ledger.
fn is_exempt(member: &Member) -> bool {
    EXEMPT.contains(&member.name.as_str()) || member.is_spike()
}

/// The banned-crate half, which applies to every manifest without exception.
fn banned_in(dependencies: &[Dependency]) -> Vec<String> {
    dependencies
        .iter()
        .filter(|dependency| {
            BANNED_PREFIXES.iter().any(|banned| {
                dependency.name == *banned
                    || dependency.name.starts_with(&format!("{banned}-"))
                    || dependency.name.starts_with(&format!("{banned}_"))
            })
        })
        .map(|dependency| {
            format!(
                "names `{}` in [{}]; the reference implementation is read, never linked",
                dependency.name,
                dependency.section.table_name()
            )
        })
        .collect()
}
