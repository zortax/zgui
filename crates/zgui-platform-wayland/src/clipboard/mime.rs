//! Which media types mean text, and which one to ask for.

use zgui_platform::ClipboardFormat;

/// The media types this backend offers when it takes a selection, best first.
///
/// Five names for one thing, and all five are needed. The first is what every modern toolkit
/// looks for; the last three are what applications written against X11's selection conventions
/// look for, and a great many still are. Offering only the modern name means a paste into an older
/// application silently produces nothing.
pub const OFFERED: [&str; 5] = [
    "text/plain;charset=utf-8",
    "text/plain",
    "UTF8_STRING",
    "STRING",
    "TEXT",
];

/// The media type for a list of files being dragged.
pub const URI_LIST: &str = "text/uri-list";

/// The media type to ask an offer for, given what it says it has.
///
/// In preference order rather than first-match, because an offer usually lists several and the
/// difference between them is the encoding: `STRING` is Latin-1 by X11 convention and
/// `text/plain` is unspecified, so asking for either when a declared UTF-8 name is on the list
/// turns every accented character into a question mark.
pub fn best_text(offered: &[String]) -> Option<String> {
    const PREFERRED: [&str; 5] = [
        "text/plain;charset=utf-8",
        "UTF8_STRING",
        "text/plain",
        "STRING",
        "TEXT",
    ];
    PREFERRED
        .iter()
        .find(|wanted| offered.iter().any(|has| has.eq_ignore_ascii_case(wanted)))
        .map(|found| (*found).to_owned())
        .or_else(|| {
            // Nothing named, but something that is still text: a compositor or an application may
            // offer `text/plain;charset=UTF-8` with different capitalisation, or a subtype nobody
            // enumerated. Anything under the text tree can at least be shown.
            offered
                .iter()
                .find(|has| has.to_ascii_lowercase().starts_with("text/"))
                .cloned()
        })
}

/// Whether an offer carries a list of files.
pub fn has_files(offered: &[String]) -> bool {
    offered.iter().any(|has| has.eq_ignore_ascii_case(URI_LIST))
}

/// Whether this backend can answer a request for `format` at all.
///
/// Only plain text crosses this boundary, which is what the contract promises and no more. Markup,
/// images and file lists on the *clipboard* are refused with a reason a caller can fall back from,
/// rather than answered with an empty value it cannot tell from an empty clipboard. Files being
/// dragged are a different path and do arrive.
pub const fn is_supported(format: ClipboardFormat) -> bool {
    matches!(format, ClipboardFormat::Text)
}

#[cfg(test)]
mod tests {
    use super::{best_text, has_files, is_supported};
    use zgui_platform::ClipboardFormat;

    fn offered(types: &[&str]) -> Vec<String> {
        types.iter().map(|name| (*name).to_owned()).collect()
    }

    #[test]
    fn an_offer_with_nothing_on_it_has_nothing_to_ask_for() {
        assert_eq!(best_text(&offered(&[])), None);
        assert_eq!(best_text(&offered(&["image/png"])), None);
    }

    #[test]
    fn a_declared_encoding_wins_over_one_that_has_to_be_guessed() {
        // `STRING` is Latin-1 by convention and `text/plain` says nothing, so taking either while
        // a declared UTF-8 name is on the list turns every accented character into a question mark.
        let both = offered(&["STRING", "text/plain", "text/plain;charset=utf-8"]);
        assert_eq!(
            best_text(&both).as_deref(),
            Some("text/plain;charset=utf-8")
        );
        let older = offered(&["STRING", "TEXT"]);
        assert_eq!(best_text(&older).as_deref(), Some("STRING"));
    }

    #[test]
    fn a_name_in_another_case_is_still_that_name() {
        let shouted = offered(&["TEXT/PLAIN;CHARSET=UTF-8"]);
        assert_eq!(
            best_text(&shouted).as_deref(),
            Some("text/plain;charset=utf-8")
        );
    }

    #[test]
    fn a_text_subtype_nobody_enumerated_is_still_text() {
        let unusual = offered(&["text/markdown"]);
        assert_eq!(best_text(&unusual).as_deref(), Some("text/markdown"));
    }

    #[test]
    fn a_list_of_files_is_recognised_whatever_its_case() {
        assert!(has_files(&offered(&["text/uri-list"])));
        assert!(has_files(&offered(&["TEXT/URI-LIST"])));
        assert!(!has_files(&offered(&["text/plain"])));
    }

    #[test]
    fn only_plain_text_crosses_the_clipboard_boundary() {
        assert!(is_supported(ClipboardFormat::Text));
        assert!(!is_supported(ClipboardFormat::Html));
        assert!(!is_supported(ClipboardFormat::Image));
        assert!(!is_supported(ClipboardFormat::FileList));
    }
}
