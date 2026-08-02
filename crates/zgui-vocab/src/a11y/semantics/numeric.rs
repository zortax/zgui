//! The numeric properties of a control whose value is a number.

/// The value of a control that measures rather than names: a slider, a progress bar, a gauge.
///
/// All five are optional and independent, because a control may know some of them and not others.
/// A progress bar of unknown duration has no maximum; a slider that can only be dragged has no
/// step.
///
/// ```
/// use zgui_vocab::Numeric;
///
/// let volume = Numeric { value: Some(0.5), min: Some(0.0), max: Some(1.0), ..Numeric::default() };
/// assert!(volume.is_set());
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Numeric {
    /// The current value.
    pub value: Option<f64>,
    /// The smallest value the control accepts.
    pub min: Option<f64>,
    /// The largest value the control accepts.
    pub max: Option<f64>,
    /// How far one increment or decrement moves the value.
    pub step: Option<f64>,
    /// How far a coarse increment — a page up, a click on the track — moves the value.
    pub jump: Option<f64>,
}

impl Numeric {
    /// Whether any numeric property is set.
    pub fn is_set(&self) -> bool {
        self.value.is_some()
            || self.min.is_some()
            || self.max.is_some()
            || self.step.is_some()
            || self.jump.is_some()
    }
}

/// Where an element sits in a set of similar ones, for sets a consumer cannot count itself.
///
/// A consumer counts the children it can see. When a list is virtualised, or a tree is partly
/// collapsed, what it can see is not the whole set, and announcing "item 3 of 12" from the visible
/// children would be wrong. These three properties are how the element that *does* know says so.
///
/// ```
/// use zgui_vocab::SetPosition;
///
/// let row = SetPosition { position_in_set: Some(3), size_of_set: Some(1_000), level: None };
/// assert!(row.is_set());
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SetPosition {
    /// This element's one-based position among its siblings in the set.
    pub position_in_set: Option<usize>,
    /// How many elements the set holds in total.
    pub size_of_set: Option<usize>,
    /// How deep this element is nested, counting from one.
    pub level: Option<usize>,
}

impl SetPosition {
    /// Whether any position property is set.
    pub fn is_set(&self) -> bool {
        self.position_in_set.is_some() || self.size_of_set.is_some() || self.level.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::{Numeric, SetPosition};

    #[test]
    fn an_unset_group_reports_itself_unset() {
        assert!(!Numeric::default().is_set());
        assert!(!SetPosition::default().is_set());
    }

    #[test]
    fn one_set_property_is_enough_to_be_set() {
        assert!(
            Numeric {
                jump: Some(10.0),
                ..Numeric::default()
            }
            .is_set()
        );
        assert!(
            SetPosition {
                level: Some(2),
                ..SetPosition::default()
            }
            .is_set()
        );
    }
}
