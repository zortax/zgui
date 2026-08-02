//! A scanner over the one markup subset a layout reference test is allowed to use.
//!
//! # This is a converter, not a parser the framework ships
//!
//! Nothing downstream ever sees markup: a suite is converted **once**, ahead of any run, into the
//! declarative format the runner reads. The rule that makes that safe is that this refuses
//! everything it does not fully understand. There is no error recovery, no implied end tag, no
//! entity table beyond five names and no attribute it does not know — a document using any of them
//! is reported unconvertible and counted as such. A converter that guessed would be a markup engine
//! growing inside a project whose whole point is not having one.

use crate::zdoc::source::Element;

/// The element names a converted test may use.
const ALLOWED: [&str; 4] = ["div", "span", "img", "body"];

/// Why a document could not be converted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unconvertible {
    /// What was found that this scanner does not accept.
    pub reason: String,
}

impl core::fmt::Display for Unconvertible {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.reason)
    }
}

/// What one document converted to.
#[derive(Clone, Debug, PartialEq)]
pub struct Converted {
    /// The style sheet, gathered from every `style` element in source order.
    pub css: String,
    /// The body, as one element tree.
    pub root: Element,
    /// The document this one says it must match, if it names one.
    pub reference: Option<String>,
}

/// Converts one reference test.
///
/// # Errors
///
/// Returns [`Unconvertible`] naming the first construct outside the subset.
pub fn convert(source: &str) -> Result<Converted, Unconvertible> {
    let mut scanner = Scanner {
        rest: source,
        css: String::new(),
        reference: None,
    };
    let mut stack = vec![Element::new("body")];
    scanner.skip_prologue()?;
    while let Some(token) = scanner.next_token()? {
        match token {
            Token::Open { name, classes, .. } => {
                let mut element = Element::new(name);
                element.classes = classes;
                stack.push(element);
            }
            Token::SelfClosing {
                name,
                classes,
                size,
            } => {
                let mut element = Element::new(name);
                element.classes = classes;
                element.replaced = size;
                stack
                    .last_mut()
                    .expect("a root is always open")
                    .children
                    .push(element);
            }
            Token::Close(name) => {
                if stack.len() == 1 {
                    if name == "body" {
                        break;
                    }
                    return Err(Unconvertible {
                        reason: format!("`</{name}>` closes an element that is not open"),
                    });
                }
                let finished = stack.pop().expect("checked above");
                if finished.name != name {
                    return Err(Unconvertible {
                        reason: format!("`</{name}>` closes an open `<{}>`", finished.name),
                    });
                }
                stack
                    .last_mut()
                    .expect("a root is always open")
                    .children
                    .push(finished);
            }
            Token::Text(text) => {
                let element = stack.last_mut().expect("a root is always open");
                if element.text.is_some() {
                    return Err(Unconvertible {
                        reason: format!("`<{}>` holds text on both sides of a child", element.name),
                    });
                }
                element.text = Some(text);
            }
        }
    }
    while stack.len() > 1 {
        let finished = stack.pop().expect("checked");
        stack
            .last_mut()
            .expect("a root is always open")
            .children
            .push(finished);
    }
    Ok(Converted {
        css: scanner.css,
        root: stack.pop().expect("the root"),
        reference: scanner.reference,
    })
}

/// One piece of the document.
enum Token {
    /// An opening tag.
    Open {
        /// The element name.
        name: String,
        /// Its classes.
        classes: Vec<String>,
    },
    /// A tag that opens and closes at once.
    SelfClosing {
        /// The element name.
        name: String,
        /// Its classes.
        classes: Vec<String>,
        /// Its declared natural size.
        size: Option<(f32, f32)>,
    },
    /// A closing tag.
    Close(String),
    /// Collapsed text content.
    Text(String),
}

/// A cursor over the document.
struct Scanner<'a> {
    /// What is left to read.
    rest: &'a str,
    /// The style sheets gathered so far.
    css: String,
    /// The reference this document says it matches.
    reference: Option<String>,
}

impl Scanner<'_> {
    /// Consumes everything up to the body, gathering the style sheets and the reference link.
    fn skip_prologue(&mut self) -> Result<(), Unconvertible> {
        while let Some(open) = self.rest.find('<') {
            let tail = &self.rest[open..];
            if let Some(after) = tail.strip_prefix("<style") {
                let start = after.find('>').ok_or_else(|| unclosed("style"))?;
                let end = after.find("</style>").ok_or_else(|| unclosed("style"))?;
                self.css.push_str(&after[start + 1..end]);
                self.css.push('\n');
                self.rest = &after[end + "</style>".len()..];
                continue;
            }
            if tail.starts_with("<link") {
                let close = tail.find('>').ok_or_else(|| unclosed("link"))?;
                let tag = &tail[..close];
                if tag.contains("rel=\"match\"") || tag.contains("rel=match") {
                    self.reference = attribute(tag, "href");
                }
                self.rest = &tail[close + 1..];
                continue;
            }
            if tail.starts_with("<body") {
                let close = tail.find('>').ok_or_else(|| unclosed("body"))?;
                self.rest = &tail[close + 1..];
                return Ok(());
            }
            let close = tail.find('>').ok_or_else(|| unclosed("a tag"))?;
            self.rest = &tail[close + 1..];
        }
        Err(Unconvertible {
            reason: "the document has no `<body>`".to_owned(),
        })
    }

    /// The next token, or nothing at the end of the document.
    fn next_token(&mut self) -> Result<Option<Token>, Unconvertible> {
        let Some(open) = self.rest.find('<') else {
            self.rest = "";
            return Ok(None);
        };
        let text = collapse(&self.rest[..open]);
        if !text.is_empty() {
            self.rest = &self.rest[open..];
            return Ok(Some(Token::Text(text)));
        }
        let tail = &self.rest[open..];
        let close = tail.find('>').ok_or_else(|| unclosed("a tag"))?;
        let tag = tail[1..close].trim();
        self.rest = &tail[close + 1..];

        if let Some(name) = tag.strip_prefix('/') {
            return Ok(Some(Token::Close(name.trim().to_owned())));
        }
        let self_closing = tag.ends_with('/');
        let tag = tag.trim_end_matches('/').trim_end();
        let name = tag.split_whitespace().next().unwrap_or_default().to_owned();
        if !ALLOWED.contains(&name.as_str()) {
            return Err(Unconvertible {
                reason: format!("`<{name}>` is outside the convertible subset"),
            });
        }
        if attribute(tag, "style").is_some() {
            return Err(Unconvertible {
                reason: format!(
                    "`<{name}>` carries a `style` attribute, which has no cascade here"
                ),
            });
        }
        let classes = attribute(tag, "class")
            .map(|value| value.split_whitespace().map(str::to_owned).collect())
            .unwrap_or_default();
        let size = match name.as_str() {
            "img" => Some((
                attribute(tag, "width")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(0.0),
                attribute(tag, "height")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(0.0),
            )),
            _ => None,
        };
        Ok(Some(if self_closing || name == "img" {
            Token::SelfClosing {
                name,
                classes,
                size,
            }
        } else {
            Token::Open { name, classes }
        }))
    }
}

/// One attribute's value, in either quoting style.
fn attribute(tag: &str, name: &str) -> Option<String> {
    let at = tag.find(&format!("{name}="))?;
    let after = &tag[at + name.len() + 1..];
    let mut characters = after.chars();
    match characters.next()? {
        quote @ ('"' | '\'') => after[1..].split(quote).next().map(str::to_owned),
        _ => after.split_whitespace().next().map(str::to_owned),
    }
}

/// White space collapsed the way a markup document would collapse it.
fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The error for a tag that never closes.
fn unclosed(what: &str) -> Unconvertible {
    Unconvertible {
        reason: format!("`<{what}>` is never closed"),
    }
}

#[cfg(test)]
mod tests {
    use super::convert;

    /// A document in the subset converts to the tree it describes.
    #[test]
    fn a_convertible_document_converts() {
        let converted = convert(
            "<!DOCTYPE html>\n\
             <link rel=\"match\" href=\"ref.html\">\n\
             <style>.a { display: flex }</style>\n\
             <body>\n\
             <div class=\"a b\">text<span></span></div>\n\
             <img width=\"40\" height=\"30\">\n\
             </body>",
        )
        .expect("convertible");

        assert_eq!(converted.reference.as_deref(), Some("ref.html"));
        assert_eq!(converted.css.trim(), ".a { display: flex }");
        assert_eq!(converted.root.children.len(), 2);
        assert_eq!(converted.root.children[0].classes, ["a", "b"]);
        assert_eq!(converted.root.children[0].text.as_deref(), Some("text"));
        assert_eq!(converted.root.children[1].replaced, Some((40.0, 30.0)));
    }

    /// Everything outside the subset is refused by name.
    ///
    /// The refusals are the point. A converter that quietly dropped a `<table>` or a `style`
    /// attribute would produce a document that lays out differently from the test it came from,
    /// and the suite's pass rate would then measure the converter.
    #[test]
    fn everything_outside_the_subset_is_refused() {
        for (source, expected) in [
            (
                "<body><table></table></body>",
                "outside the convertible subset",
            ),
            (
                "<body><div style=\"width: 1px\"></div></body>",
                "`style` attribute",
            ),
            ("<body></div></body>", "closes an element that is not open"),
            ("<body><div><span></div></body>", "closes an open"),
            ("<style>.a {}", "never closed"),
            ("<div></div>", "no `<body>`"),
        ] {
            let error = convert(source).expect_err(source);
            assert!(
                error.reason.contains(expected),
                "{source:?} reported {error} rather than {expected:?}",
            );
        }
    }
}
