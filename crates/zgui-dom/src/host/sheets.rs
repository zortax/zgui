//! Where the source of an imported or linked stylesheet comes from.

use zgui_vocab::SharedString;

/// What a loader has to say about one stylesheet request.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum SheetRequest {
    /// The source text, available now.
    ///
    /// The parser continues with it in the same call, which is what `@import` needs: the imported
    /// rules take the position of the `@import` in the importing sheet, and a rule set cannot be
    /// spliced into the middle of a sheet afterwards.
    Ready(SharedString),
    /// The source is not available yet.
    ///
    /// The `@import` contributes nothing for now. Delivering the text later is the loader's own
    /// business, and it takes effect by replacing the sheet rather than by patching one already
    /// parsed.
    Pending,
    /// The loader refuses this request.
    ///
    /// The `@import` is dropped and reported as a parse error, which is the same outcome as having
    /// no loader at all.
    Rejected,
}

/// Loads the source of stylesheets a document refers to rather than carries.
///
/// A document core that could fetch a URL would need a network stack, a cache, a security policy
/// and an idea of what a URL is; a consumer that wants `@import` already has all four. Without a
/// loader installed every request is refused, so `@import` is reported as an error at parse time
/// rather than silently doing nothing.
pub trait SheetLoader: Send + Sync + 'static {
    /// Resolves `href` against `base` and produces the sheet's source.
    ///
    /// Both are text rather than a parsed URL type, because resolving one against the other is the
    /// loader's decision: a consumer with a document base, a cache and a security policy is the
    /// only party that can say what `../theme.css` means, and a document core that took a position
    /// on it would have to be overridden rather than implemented.
    ///
    /// Called on the thread that is parsing, and never from a style worker.
    fn load(&self, base: &str, href: &str) -> SheetRequest;
}

/// The loader zgui installs by default, which refuses everything.
pub struct NoSheetLoader;

impl SheetLoader for NoSheetLoader {
    fn load(&self, _base: &str, _href: &str) -> SheetRequest {
        SheetRequest::Rejected
    }
}
