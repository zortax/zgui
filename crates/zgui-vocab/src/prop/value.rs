//! The value half of an imperative property.

use crate::text::SharedString;

/// The value of an imperative property.
///
/// The set is small on purpose. A property is read by the element's own behaviour, so its value
/// has to be something every backend can carry — a native tree, a document object model, a test
/// recorder — and a variant that only one of them could express would break that. Anything richer
/// belongs behind a property whose value is text the owner parses.
///
/// [`PropValue::Unset`] is a value rather than the absence of one: writing it is how a property is
/// removed, which keeps a write and an erase on the same path.
///
/// ```
/// use zgui_vocab::PropValue;
///
/// let value = PropValue::from("hello");
/// assert_eq!(value.as_str(), Some("hello"));
/// assert_eq!(PropValue::from(true).as_bool(), Some(true));
/// assert!(PropValue::Unset.is_unset());
/// ```
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub enum PropValue {
    /// The property is not set, and writing this removes it.
    #[default]
    Unset,
    /// A flag, such as whether a field is currently obscured.
    Bool(bool),
    /// A whole number, such as a selection offset.
    Integer(i64),
    /// A real number, such as a media element's volume.
    Number(f64),
    /// Text, such as a field's current value.
    Text(SharedString),
}

impl PropValue {
    /// Whether this is the absent value.
    pub fn is_unset(&self) -> bool {
        matches!(self, Self::Unset)
    }

    /// The flag, when this is one.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            _ => None,
        }
    }

    /// The whole number, when this is one.
    pub fn as_integer(&self) -> Option<i64> {
        match self {
            Self::Integer(value) => Some(*value),
            _ => None,
        }
    }

    /// The real number, when this is one — including a whole number widened to it.
    pub fn as_number(&self) -> Option<f64> {
        match self {
            Self::Number(value) => Some(*value),
            Self::Integer(value) => Some(*value as f64),
            _ => None,
        }
    }

    /// The text, when this is text.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Text(value) => Some(value.as_str()),
            _ => None,
        }
    }
}

impl From<bool> for PropValue {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl From<i64> for PropValue {
    fn from(value: i64) -> Self {
        Self::Integer(value)
    }
}

impl From<f64> for PropValue {
    fn from(value: f64) -> Self {
        Self::Number(value)
    }
}

impl From<SharedString> for PropValue {
    fn from(value: SharedString) -> Self {
        Self::Text(value)
    }
}

impl From<&str> for PropValue {
    fn from(value: &str) -> Self {
        Self::Text(SharedString::from(value))
    }
}

/// A value that was built rather than written down.
///
/// A property whose text is a constant is a `&str` and needs no allocation at all; one that was
/// assembled — a document read from a file, a path notation formatted from numbers — arrives owned,
/// and without this the only way to set it is to hand it back to [`SharedString`] by hand at every
/// call site.
impl From<String> for PropValue {
    fn from(value: String) -> Self {
        Self::Text(SharedString::from(value))
    }
}

impl<T: Into<PropValue>> From<Option<T>> for PropValue {
    fn from(value: Option<T>) -> Self {
        value.map_or(Self::Unset, Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::PropValue;

    #[test]
    fn absence_is_a_value_so_a_write_can_erase() {
        let value: PropValue = Option::<&str>::None.into();
        assert!(value.is_unset());
        assert_eq!(PropValue::default(), PropValue::Unset);
    }

    #[test]
    fn a_whole_number_reads_as_a_real_one() {
        assert_eq!(PropValue::Integer(3).as_number(), Some(3.0));
        assert_eq!(PropValue::Number(3.5).as_integer(), None);
    }

    #[test]
    fn a_mismatched_read_is_none_rather_than_a_coercion() {
        assert_eq!(PropValue::from(true).as_str(), None);
        assert_eq!(PropValue::from("true").as_bool(), None);
    }
}
