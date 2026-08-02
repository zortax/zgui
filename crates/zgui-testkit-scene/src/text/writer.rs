//! The indented line writer both a scene transcript and a tree dump are built with.

/// Collects indented lines into one diffable block of text.
///
/// Two properties are the whole point, and both are enforced here rather than trusted to callers.
/// Every line ends with a newline, so a block always ends with one and a golden file never differs
/// from a rendering by a trailing byte. And no line may *contain* a newline: a value that carried
/// one would silently invent lines that no nesting level explains, so newlines are escaped on the
/// way in.
///
/// ```
/// use zgui_testkit_scene::text::Writer;
///
/// let mut writer = Writer::new();
/// writer.line("scene viewport=8x8");
/// writer.nested("primitives 1", |writer| {
///     writer.line("quad order=0");
/// });
///
/// assert_eq!(writer.finish(), "scene viewport=8x8\nprimitives 1\n  quad order=0\n");
/// ```
#[derive(Clone, Debug, Default)]
pub struct Writer {
    /// The text so far.
    out: String,
    /// How many levels deep the next line is written at.
    depth: usize,
}

impl Writer {
    /// How many spaces one level of nesting costs.
    pub const INDENT: usize = 2;

    /// A writer with nothing written and nothing nested.
    pub fn new() -> Self {
        Self::default()
    }

    /// Writes one line at the current depth.
    pub fn line(&mut self, text: &str) {
        for _ in 0..self.depth * Self::INDENT {
            self.out.push(' ');
        }
        for character in text.chars() {
            match character {
                '\n' => self.out.push_str("\\n"),
                '\r' => self.out.push_str("\\r"),
                other => self.out.push(other),
            }
        }
        self.out.push('\n');
    }

    /// Writes `header` and then `body` one level deeper.
    pub fn nested(&mut self, header: &str, body: impl FnOnce(&mut Self)) {
        self.line(header);
        self.depth += 1;
        body(self);
        self.depth -= 1;
    }

    /// How deep the next line will be written.
    pub fn depth(&self) -> usize {
        self.depth
    }

    /// Whether nothing has been written at all.
    pub fn is_empty(&self) -> bool {
        self.out.is_empty()
    }

    /// The finished text.
    pub fn finish(self) -> String {
        self.out
    }
}

#[cfg(test)]
mod tests {
    use super::Writer;

    #[test]
    fn nesting_indents_and_unwinds() {
        let mut writer = Writer::new();
        writer.nested("a", |writer| {
            writer.nested("b", |writer| writer.line("c"));
            writer.line("d");
        });
        writer.line("e");
        assert_eq!(writer.finish(), "a\n  b\n    c\n  d\ne\n");
    }

    #[test]
    fn a_value_carrying_a_newline_cannot_invent_a_line() {
        let mut writer = Writer::new();
        writer.line("text=\"one\ntwo\"");
        let out = writer.finish();
        assert_eq!(out.lines().count(), 1);
        assert!(out.contains("one\\ntwo"));
    }

    #[test]
    fn an_empty_writer_produces_no_text_at_all() {
        assert!(Writer::new().is_empty());
        assert_eq!(Writer::new().finish(), "");
    }
}
