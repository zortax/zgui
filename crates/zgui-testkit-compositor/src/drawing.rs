//! What a frame draws, read back from its transcript.

use core::fmt;

use compositor_capture::Transcript;

/// What one frame draws: its instances in draw order, read from the capture transcript.
///
/// Each instance is one line of the transcript. Its fields are the `name=value` words of the line,
/// where a value runs to the next space outside brackets and parentheses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Drawing {
    transcript: Transcript,
    instances: Vec<Instance>,
    objects: Vec<Instance>,
}

/// One instance a frame draws, such as a quad or a glyph sprite.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Instance {
    kind: String,
    text: String,
}

impl Drawing {
    /// Reads the instances and the objects of a capture's drawing transcript.
    pub fn of(transcript: Transcript) -> Self {
        let instances = transcript
            .lines()
            .filter_map(|line| line.strip_prefix("    "))
            .filter(|line| !line.starts_with(' '))
            .map(Instance::of)
            .collect();
        let objects = transcript
            .lines()
            .filter_map(|line| line.strip_prefix("  "))
            .map(Instance::of)
            .filter(|object| OBJECTS.contains(&object.kind()))
            .collect();
        Self {
            transcript,
            instances,
            objects,
        }
    }

    /// Returns the objects of `kind` the frame draws one at a time, such as `backdrop`,
    /// `external`, `group_begin` or `group_end`, in draw order.
    pub fn objects<'a>(&'a self, kind: &'a str) -> impl Iterator<Item = &'a Instance> + 'a {
        self.objects
            .iter()
            .filter(move |object| object.kind == kind)
    }

    /// Returns the transcript.
    pub fn transcript(&self) -> &Transcript {
        &self.transcript
    }

    /// Returns every instance, in draw order.
    pub fn instances(&self) -> &[Instance] {
        &self.instances
    }

    /// Returns the instances of `kind`, such as `quad`, `shadow`, `line`, `mono`, `subpixel` or
    /// `color`, in draw order.
    pub fn of_kind<'a>(&'a self, kind: &'a str) -> impl Iterator<Item = &'a Instance> + 'a {
        self.instances
            .iter()
            .filter(move |instance| instance.kind == kind)
    }

    /// Returns the quads, in draw order.
    pub fn quads(&self) -> Vec<&Instance> {
        self.of_kind("quad").collect()
    }

    /// Returns the glyph sprites of every kind, in draw order.
    pub fn glyphs(&self) -> Vec<&Instance> {
        self.instances
            .iter()
            .filter(|instance| matches!(instance.kind(), "mono" | "subpixel" | "color"))
            .collect()
    }
}

impl fmt::Display for Drawing {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.transcript, formatter)
    }
}

/// The kinds of the objects a frame draws one at a time, each a line of its own.
const OBJECTS: [&str; 4] = ["backdrop", "external", "group_begin", "group_end"];

impl Instance {
    /// Reads one line of a transcript.
    fn of(line: &str) -> Self {
        Self {
            kind: line.split(' ').next().unwrap_or_default().to_owned(),
            text: line.to_owned(),
        }
    }

    /// Returns the instance's kind, the first word of its line.
    pub fn kind(&self) -> &str {
        &self.kind
    }

    /// Returns the instance's whole line.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Returns the value of the field `name`, or `None` when the line has no such field.
    pub fn field(&self, name: &str) -> Option<&str> {
        words(&self.text).find_map(|word| {
            let (key, value) = word.split_once('=')?;
            (key == name).then_some(value)
        })
    }

    /// Returns the paint in the field `name` as written, such as `solid premul[1, 0, 0, 1]` or
    /// `sides [...]`, or `None` when the line has no such field.
    ///
    /// The paint runs to the next field. The geometry of a gradient is written as fields of its
    /// own, so a gradient paint reads as its shape alone, such as `linear`.
    pub fn paint(&self, name: &str) -> Option<String> {
        let mut words = words(&self.text)
            .skip_while(|word| word.split_once('=').is_none_or(|(key, _)| key != name));
        let first = words.next()?.split_once('=')?.1.to_owned();
        let rest = words.take_while(|word| !word.contains('='));
        Some(rest.fold(first, |paint, word| paint + " " + word))
    }

    /// Reports whether the line holds the bare word `flag`, such as `inset`.
    pub fn has(&self, flag: &str) -> bool {
        words(&self.text).any(|word| word == flag)
    }

    /// Returns the field `name` as the rectangle `rect(x, y, width, height)` it holds.
    pub fn rect(&self, name: &str) -> Option<[f32; 4]> {
        let inner = self.field(name)?.strip_prefix("rect(")?.strip_suffix(')')?;
        array(inner)
    }

    /// Returns the bounds.
    ///
    /// # Panics
    ///
    /// Panics when the instance has no bounds.
    pub fn bounds(&self) -> [f32; 4] {
        self.rect("bounds")
            .unwrap_or_else(|| panic!("no bounds in `{}`", self.text))
    }

    /// Returns the premultiplied colour of the solid paint or colour in the field `name`.
    ///
    /// A solid paint is written `solid premul[r, g, b, a]` and a colour `premul[r, g, b, a]`.
    pub fn color(&self, name: &str) -> Option<[f32; 4]> {
        let at = self.text.find(&format!(" {name}="))? + name.len() + 2;
        let rest = self.text[at..]
            .strip_prefix("solid ")
            .unwrap_or(&self.text[at..]);
        let inner = rest.strip_prefix("premul[")?;
        array(&inner[..inner.find(']')?])
    }
}

/// Returns the words of `line` split at spaces outside brackets and parentheses.
fn words(line: &str) -> impl Iterator<Item = &str> {
    let mut depth = 0_i32;
    let mut start = 0;
    let mut words = Vec::new();
    for (at, byte) in line.bytes().enumerate() {
        match byte {
            b'[' | b'(' => depth += 1,
            b']' | b')' => depth -= 1,
            b' ' if depth == 0 => {
                words.push(&line[start..at]);
                start = at + 1;
            }
            _ => {}
        }
    }
    words.push(&line[start..]);
    words.into_iter()
}

/// Returns the four comma-separated numbers of `list`.
fn array(list: &str) -> Option<[f32; 4]> {
    let mut out = [0.0; 4];
    let mut numbers = list.split(',').map(|number| number.trim().parse::<f32>());
    for slot in &mut out {
        *slot = numbers.next()?.ok()?;
    }
    numbers.next().is_none().then_some(out)
}
