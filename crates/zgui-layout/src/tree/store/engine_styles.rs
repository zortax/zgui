//! The engine styles boxes hold, interned so that boxes that agree share one entry.
//!
//! A box's engine style is its lowering's template with the box's variant patched in, and most
//! boxes of a document agree with many others — every cell of a table, every row of a list. One
//! entry per distinct value keeps the column of a 10 000-box document small, and lets the engine
//! borrow a style by reference for as long as it needs it.

use cephal::compute::plan::StylePlan;
use rustc_hash::FxHashMap;

/// One interned engine style.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct EngineStyleId(u32);

#[derive(Debug)]
struct Entry {
    style: cephal::Style,
    /// [`cephal::Style::parent_size_dependency`], computed once per distinct style because the
    /// engine asks for it on every child query.
    dependency: cephal::Size<bool>,
    /// The style's lengths resolved once, when none of them needs a parent size; the engine's
    /// algorithms and the leaf shell read these instead of resolving per question.
    plan: Option<StylePlan>,
    refs: u32,
}

/// The interned engine styles.
#[derive(Debug, Default)]
pub(crate) struct EngineStyles {
    entries: Vec<Option<Entry>>,
    free: Vec<u32>,
    by_value: FxHashMap<cephal::Style, u32>,
}

impl EngineStyles {
    /// The entry for `style`, made if none holds that value; the caller holds one reference.
    pub(crate) fn intern(&mut self, style: cephal::Style) -> EngineStyleId {
        if let Some(&index) = self.by_value.get(&style) {
            self.entries[index as usize]
                .as_mut()
                .expect("an indexed entry is live")
                .refs += 1;
            return EngineStyleId(index);
        }
        let entry = Entry {
            dependency: style.parent_size_dependency(),
            plan: StylePlan::from_style(&style),
            style: style.clone(),
            refs: 1,
        };
        let index = match self.free.pop() {
            Some(index) => {
                self.entries[index as usize] = Some(entry);
                index
            }
            None => {
                let index =
                    u32::try_from(self.entries.len()).expect("fewer than 2^32 engine styles");
                self.entries.push(Some(entry));
                index
            }
        };
        self.by_value.insert(style, index);
        EngineStyleId(index)
    }

    /// Gives one reference back; the entry is dropped with its last holder.
    pub(crate) fn release(&mut self, id: EngineStyleId) {
        let index = id.0 as usize;
        let Some(entry) = self.entries[index].as_mut() else {
            debug_assert!(false, "released an engine style twice");
            return;
        };
        entry.refs -= 1;
        if entry.refs > 0 {
            return;
        }
        let entry = self.entries[index].take().expect("checked above");
        self.by_value.remove(&entry.style);
        self.free.push(id.0);
    }

    /// The style one identifier names.
    pub(crate) fn get(&self, id: EngineStyleId) -> &cephal::Style {
        &self.entries[id.0 as usize]
            .as_ref()
            .expect("a live engine style")
            .style
    }

    /// The style one identifier names, and the parent sizes it resolves against.
    pub(crate) fn with_dependency(
        &self,
        id: EngineStyleId,
    ) -> (&cephal::Style, cephal::Size<bool>) {
        let entry = self.entries[id.0 as usize]
            .as_ref()
            .expect("a live engine style");
        (&entry.style, entry.dependency)
    }

    /// The resolved lengths of the style one identifier names, if it has no relative ones.
    pub(crate) fn plan(&self, id: EngineStyleId) -> Option<&StylePlan> {
        self.entries[id.0 as usize]
            .as_ref()
            .expect("a live engine style")
            .plan
            .as_ref()
    }

    /// How many distinct styles are held.
    pub(crate) fn live(&self) -> usize {
        self.by_value.len()
    }
}

#[cfg(test)]
mod tests {
    use super::EngineStyles;

    #[test]
    fn equal_styles_share_one_entry_until_the_last_holder_lets_go() {
        let mut styles = EngineStyles::default();
        let first = styles.intern(cephal::Style::DEFAULT);
        let second = styles.intern(cephal::Style::DEFAULT);
        assert_eq!(first, second);
        assert_eq!(styles.live(), 1);
        let mut other = cephal::Style::DEFAULT;
        other.flex_grow = 1.0;
        let third = styles.intern(other);
        assert_ne!(first, third);
        assert_eq!(styles.live(), 2);
        styles.release(first);
        assert_eq!(styles.live(), 2, "one holder is left");
        styles.release(second);
        assert_eq!(styles.live(), 1);
        let reused = styles.intern(cephal::Style::DEFAULT);
        assert_eq!(reused, first, "a dead entry grows the table forever");
    }
}
