//! The relations one element declares to others.

use accesskit::NodeId;

/// Which other elements this one is related to, and how.
///
/// A relation is the only part of an element's meaning that is not about the element itself, and
/// several whole categories of control cannot be described without one. A field is named by a
/// label that is not its ancestor; a tab controls a panel that is nowhere near it in the tree; a
/// trigger owns a pop-up surface that the framework had to move to the top of the window to draw
/// it. In every case the tree structure says the wrong thing and the relation says the right one.
///
/// Two shapes appear here, and the difference is not cosmetic. A relation that can name several
/// targets — a field labelled by two separate pieces of text — is a list, in the order the targets
/// should be read. A relation that names exactly one is a single identifier.
///
/// ```
/// use accesskit::NodeId;
/// use zgui_vocab::Relations;
///
/// let mut relations = Relations::default();
/// relations.labelled_by.push(NodeId(7));
/// relations.error_message = Some(NodeId(9));
///
/// assert!(!relations.is_empty());
/// assert_eq!(relations.targets().count(), 2);
/// ```
///
/// # Targets must exist
///
/// A relation naming an element that is not in the tree is not a missing feature but a broken
/// one: a consumer resolves the identifier without checking it. [`Relations::targets`] exists so
/// that whatever builds the tree can filter the whole set through one existence test rather than
/// remembering to check each field.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Relations {
    /// The elements whose text names this one, in reading order.
    pub labelled_by: Vec<NodeId>,
    /// The elements whose text describes this one at greater length than its name does.
    pub described_by: Vec<NodeId>,
    /// The elements whose content or presence this one governs.
    pub controls: Vec<NodeId>,
    /// The elements that belong to this one despite not being its children in the tree.
    ///
    /// This is what makes a pop-up surface reachable from the control that opened it, when the
    /// surface had to be drawn somewhere else entirely.
    pub owns: Vec<NodeId>,
    /// The other members of this element's mutually exclusive group.
    pub radio_group: Vec<NodeId>,
    /// The descendant that is currently active, for a composite control that keeps focus on
    /// itself while moving a selection inside it.
    pub active_descendant: Option<NodeId>,
    /// The element this one is the pop-up surface for.
    pub popup_for: Option<NodeId>,
    /// The element holding the message explaining why this control's value is rejected.
    pub error_message: Option<NodeId>,
}

impl Relations {
    /// Whether no relation at all is declared.
    pub fn is_empty(&self) -> bool {
        self.labelled_by.is_empty()
            && self.described_by.is_empty()
            && self.controls.is_empty()
            && self.owns.is_empty()
            && self.radio_group.is_empty()
            && self.active_descendant.is_none()
            && self.popup_for.is_none()
            && self.error_message.is_none()
    }

    /// Every element named by any relation, in declaration order, with duplicates kept.
    ///
    /// This is the set that has to exist in the tree.
    pub fn targets(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.labelled_by
            .iter()
            .chain(&self.described_by)
            .chain(&self.controls)
            .chain(&self.owns)
            .chain(&self.radio_group)
            .copied()
            .chain(self.active_descendant)
            .chain(self.popup_for)
            .chain(self.error_message)
    }

    /// Drops every target for which `exists` answers `false`.
    ///
    /// Building a tree that names an element which is not in it is not recoverable downstream, so
    /// the filtering happens here, once, over every relation at the same time.
    ///
    /// ```
    /// use accesskit::NodeId;
    /// use zgui_vocab::Relations;
    ///
    /// let mut relations = Relations::default();
    /// relations.labelled_by = vec![NodeId(1), NodeId(2)];
    /// relations.popup_for = Some(NodeId(3));
    ///
    /// relations.retain_targets(|id| id == NodeId(1));
    /// assert_eq!(relations.labelled_by, vec![NodeId(1)]);
    /// assert_eq!(relations.popup_for, None);
    /// ```
    pub fn retain_targets(&mut self, mut exists: impl FnMut(NodeId) -> bool) {
        for list in [
            &mut self.labelled_by,
            &mut self.described_by,
            &mut self.controls,
            &mut self.owns,
            &mut self.radio_group,
        ] {
            list.retain(|id| exists(*id));
        }
        for single in [
            &mut self.active_descendant,
            &mut self.popup_for,
            &mut self.error_message,
        ] {
            if single.is_some_and(|id| !exists(id)) {
                *single = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Relations;
    use accesskit::NodeId;

    fn populated() -> Relations {
        Relations {
            labelled_by: vec![NodeId(1), NodeId(2)],
            described_by: vec![NodeId(3)],
            controls: vec![NodeId(4)],
            owns: vec![NodeId(5)],
            radio_group: vec![NodeId(6)],
            active_descendant: Some(NodeId(7)),
            popup_for: Some(NodeId(8)),
            error_message: Some(NodeId(9)),
        }
    }

    #[test]
    fn an_empty_set_is_reported_empty() {
        assert!(Relations::default().is_empty());
        assert_eq!(Relations::default().targets().count(), 0);
        assert!(!populated().is_empty());
    }

    #[test]
    fn every_relation_field_reaches_the_target_iterator() {
        let targets: Vec<_> = populated().targets().collect();
        assert_eq!(targets, (1..=9).map(NodeId).collect::<Vec<_>>());
    }

    #[test]
    fn filtering_removes_targets_from_every_relation_shape() {
        let mut relations = populated();
        relations.retain_targets(|id| id.0 % 2 == 0);
        assert_eq!(relations.labelled_by, vec![NodeId(2)]);
        assert!(relations.described_by.is_empty());
        assert_eq!(relations.controls, vec![NodeId(4)]);
        assert_eq!(relations.active_descendant, None);
        assert_eq!(relations.popup_for, Some(NodeId(8)));
        assert_eq!(relations.error_message, None);
    }

    #[test]
    fn filtering_everything_away_leaves_an_empty_set() {
        let mut relations = populated();
        relations.retain_targets(|_| false);
        assert!(relations.is_empty());
    }
}
