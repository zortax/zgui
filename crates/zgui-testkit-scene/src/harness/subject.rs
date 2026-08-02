//! The named things a frame drew, and where their ink landed.
//!
//! A budget test says "the damage covers the subject's ink and nothing else", which needs a way to
//! ask where a named thing is. In a finished pipeline that answer comes from the fragment tree; the
//! fragment tree does not exist yet, and this crate must not grow a layout dependency to reach one.
//!
//! So the frame body *names* what it drew, and a test asks by that name. Everything above the
//! naming — damage, ink, transcripts — is real; the naming is the one part standing in for a query
//! against real geometry.

use zgui_geom::{Device, Rect};

/// One named thing a frame drew.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Subject {
    /// What the frame body called it.
    pub name: String,
    /// Everything it painted, in device pixels.
    pub ink: Rect<i32, Device>,
}

/// Everything the current frame named, in the order it was named.
#[derive(Clone, Debug, Default)]
pub struct Subjects {
    /// The entries.
    entries: Vec<Subject>,
}

impl Subjects {
    /// No subjects at all.
    pub fn new() -> Self {
        Self::default()
    }

    /// Records `name` as covering `ink`.
    ///
    /// Naming the same subject twice in one frame replaces the earlier entry rather than adding a
    /// second: a name is an identity, and two answers to one question is how a test comes to assert
    /// against whichever happened to be found first.
    pub fn record(&mut self, name: &str, ink: Rect<i32, Device>) {
        match self.entries.iter_mut().find(|held| held.name == name) {
            Some(held) => held.ink = ink,
            None => self.entries.push(Subject {
                name: name.to_owned(),
                ink,
            }),
        }
    }

    /// The subject called `name`, if the frame drew one.
    pub fn get(&self, name: &str) -> Option<&Subject> {
        self.entries.iter().find(|held| held.name == name)
    }

    /// Every subject, in the order it was named.
    pub fn all(&self) -> &[Subject] {
        &self.entries
    }

    /// Forgets everything, which is what the start of a frame does.
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

#[cfg(test)]
mod tests {
    use zgui_geom::{Point, Rect, Size};

    use super::Subjects;

    /// A device rectangle.
    fn rect(x: i32, y: i32) -> Rect<i32, zgui_geom::Device> {
        Rect::new(Point::new(x, y), Size::new(4, 4))
    }

    #[test]
    fn naming_a_subject_twice_replaces_it_rather_than_shadowing_it() {
        let mut subjects = Subjects::new();
        subjects.record("#row", rect(0, 0));
        subjects.record("#row", rect(0, 8));
        assert_eq!(subjects.all().len(), 1);
        assert_eq!(subjects.get("#row").map(|held| held.ink), Some(rect(0, 8)));
    }

    #[test]
    fn a_subject_the_frame_did_not_draw_is_absent_rather_than_empty() {
        let subjects = Subjects::new();
        assert!(subjects.get("#missing").is_none());
    }
}
