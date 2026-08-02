//! Reading the `.zdoc` text.
//!
//! Every rejection is an error naming the line, and nothing is guessed at. A converted suite is
//! machine-written, so a document that does not parse is a converter bug, and a parser that
//! recovered from one would hide it behind a test that quietly measured a different tree.

use crate::zdoc::source::{Element, Zdoc};

/// How many spaces one level of nesting costs.
const INDENT: usize = 2;

/// Why a `.zdoc` could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    /// The one-based line the problem is on.
    pub line: usize,
    /// What was wrong with it.
    pub reason: String,
}

impl core::fmt::Display for ParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "line {}: {}", self.line, self.reason)
    }
}

impl core::error::Error for ParseError {}

impl Zdoc {
    /// Reads one converted test.
    ///
    /// # Errors
    ///
    /// Returns the first [`ParseError`]: an unknown directive, a tree that is not singly rooted, an
    /// indentation step of more than one level, or an element line that is not a name, some
    /// classes, a quoted string and a natural size.
    pub fn parse(source: &str) -> Result<Self, ParseError> {
        let mut viewport = Self::DEFAULT_VIEWPORT;
        let mut css = String::new();
        let mut rows: Vec<(usize, Element, usize)> = Vec::new();
        let mut section = Section::None;

        for (offset, raw) in source.lines().enumerate() {
            let line = offset + 1;
            let trimmed = raw.trim_end();
            if trimmed.trim_start().starts_with('#') || trimmed.trim().is_empty() {
                continue;
            }
            if let Some(directive) = trimmed.strip_prefix('@') {
                section = directive_section(directive, line, &mut viewport)?;
                continue;
            }
            match section {
                Section::None => {
                    return Err(ParseError {
                        line,
                        reason: format!("`{trimmed}` is outside any section"),
                    });
                }
                Section::Css => {
                    css.push_str(trimmed);
                    css.push('\n');
                }
                Section::Tree => {
                    rows.push((indent_of(trimmed, line)?, element(trimmed, line)?, line))
                }
            }
        }

        Ok(Self {
            viewport,
            css,
            root: assemble(rows)?,
        })
    }
}

/// Which section the lines that follow belong to.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    /// Before any directive, where no line is allowed.
    None,
    /// The style sheet.
    Css,
    /// The element tree.
    Tree,
}

/// Reads one `@…` directive, which either opens a section or sets the viewport.
fn directive_section(
    directive: &str,
    line: usize,
    viewport: &mut (f32, f32),
) -> Result<Section, ParseError> {
    let mut words = directive.split_whitespace();
    match words.next() {
        Some("css") => Ok(Section::Css),
        Some("tree") => Ok(Section::Tree),
        Some("viewport") => {
            let mut number = || -> Result<f32, ParseError> {
                words
                    .next()
                    .and_then(|word| word.parse().ok())
                    .ok_or_else(|| ParseError {
                        line,
                        reason: "`@viewport` takes a width and a height in CSS pixels".to_owned(),
                    })
            };
            *viewport = (number()?, number()?);
            Ok(Section::None)
        }
        _ => Err(ParseError {
            line,
            reason: format!("`@{directive}` is not a directive"),
        }),
    }
}

/// The nesting level of one tree line.
fn indent_of(line_text: &str, line: usize) -> Result<usize, ParseError> {
    let spaces = line_text.len() - line_text.trim_start_matches(' ').len();
    if !spaces.is_multiple_of(INDENT) {
        return Err(ParseError {
            line,
            reason: format!("indented by {spaces} spaces, which is not a multiple of {INDENT}"),
        });
    }
    Ok(spaces / INDENT)
}

/// Reads one element line: a name, some classes, a quoted text and a natural size.
fn element(line_text: &str, line: usize) -> Result<Element, ParseError> {
    let mut rest = line_text.trim();
    let mut node = Element::default();

    if let Some(open) = rest.find('[') {
        let close = rest
            .rfind(']')
            .filter(|close| *close > open)
            .ok_or(ParseError {
                line,
                reason: "a natural size opens with `[` and closes with `]`".to_owned(),
            })?;
        node.replaced = Some(size(&rest[open + 1..close], line)?);
        rest = rest[..open].trim_end();
    }

    if let Some(open) = rest.find('"') {
        let close = rest
            .rfind('"')
            .filter(|close| *close > open)
            .ok_or(ParseError {
                line,
                reason: "text opens and closes with a double quote".to_owned(),
            })?;
        node.text = Some(rest[open + 1..close].to_owned());
        rest = rest[..open].trim_end();
    }

    let mut names = rest.split('.');
    node.name = names
        .next()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .ok_or(ParseError {
            line,
            reason: "an element line begins with a name".to_owned(),
        })?
        .to_owned();
    for class in names {
        if class.is_empty() {
            return Err(ParseError {
                line,
                reason: "an empty class name".to_owned(),
            });
        }
        node.classes.push(class.to_owned());
    }
    Ok(node)
}

/// Reads a `40x30` natural size.
fn size(text: &str, line: usize) -> Result<(f32, f32), ParseError> {
    let mut halves = text.split(['x', 'X']);
    let mut number = || -> Result<f32, ParseError> {
        halves
            .next()
            .map(str::trim)
            .and_then(|word| word.parse().ok())
            .ok_or_else(|| ParseError {
                line,
                reason: format!("`{text}` is not a natural size like `40x30`"),
            })
    };
    Ok((number()?, number()?))
}

/// Nests the flat list of indented rows into one tree.
fn assemble(rows: Vec<(usize, Element, usize)>) -> Result<Element, ParseError> {
    let mut stack: Vec<Element> = Vec::new();
    let mut root: Option<Element> = None;

    for (depth, node, line) in rows {
        if depth > stack.len() {
            return Err(ParseError {
                line,
                reason: format!(
                    "indented {depth} levels below a parent {} levels deep",
                    stack.len()
                ),
            });
        }
        while stack.len() > depth {
            let finished = stack.pop().expect("the stack is deeper than the depth");
            match stack.last_mut() {
                Some(parent) => parent.children.push(finished),
                None if root.is_none() => root = Some(finished),
                None => {
                    return Err(ParseError {
                        line,
                        reason: "a second root element".to_owned(),
                    });
                }
            }
        }
        if depth == 0 && root.is_some() {
            return Err(ParseError {
                line,
                reason: "a second root element".to_owned(),
            });
        }
        stack.push(node);
    }

    while let Some(finished) = stack.pop() {
        match stack.last_mut() {
            Some(parent) => parent.children.push(finished),
            None => root = Some(finished),
        }
    }

    root.ok_or(ParseError {
        line: 0,
        reason: "the document has no `@tree` section, or an empty one".to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use crate::zdoc::Zdoc;

    /// Everything the format carries survives a round through the parser.
    #[test]
    fn a_document_parses_into_the_tree_it_describes() {
        let document = Zdoc::parse(
            "# a comment\n\
             @viewport 320 240\n\
             @css\n\
             root { display: block }\n\
             @tree\n\
             root\n  \
             div.a.b \"hello\"\n    \
             span\n  \
             img [40x30]\n",
        )
        .expect("well formed");

        assert_eq!(document.viewport, (320.0, 240.0));
        assert_eq!(document.css.trim(), "root { display: block }");
        assert_eq!(document.root.name, "root");
        assert_eq!(document.root.children.len(), 2);

        let first = &document.root.children[0];
        assert_eq!(first.classes, ["a", "b"]);
        assert_eq!(first.text.as_deref(), Some("hello"));
        assert_eq!(first.children[0].name, "span");
        assert_eq!(document.root.children[1].replaced, Some((40.0, 30.0)));
        assert_eq!(document.root.count(), 4);
    }

    /// A document with no viewport is laid out in the suite's own default one.
    #[test]
    fn a_document_without_a_viewport_takes_the_default() {
        let document = Zdoc::parse("@tree\nroot\n").expect("well formed");
        assert_eq!(document.viewport, Zdoc::DEFAULT_VIEWPORT);
    }

    /// Every malformed shape is refused rather than repaired.
    ///
    /// A converter writes these files, so a parser that recovered would turn a converter bug into
    /// a test that silently measured a different tree from the one the suite specifies.
    #[test]
    fn malformed_documents_are_refused() {
        for (source, expected) in [
            ("@tree\nroot\nsecond\n", "a second root element"),
            ("@tree\nroot\n    deep\n", "indented 2 levels"),
            ("@tree\n root\n", "not a multiple of"),
            ("@nope\n", "is not a directive"),
            ("root\n", "outside any section"),
            ("@tree\nroot\n  img [40]\n", "not a natural size"),
            ("@tree\nroot..a\n", "an empty class name"),
            ("@css\nbody {}\n", "no `@tree` section"),
        ] {
            let error = Zdoc::parse(source).expect_err(source);
            assert!(
                error.reason.contains(expected),
                "{source:?} reported {error} rather than {expected:?}",
            );
        }
    }
}
