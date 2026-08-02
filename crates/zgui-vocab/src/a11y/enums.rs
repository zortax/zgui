//! The small closed enumerations a semantic property can take.
//!
//! Each is re-exported rather than restated, for the same reason the role enumeration is: these
//! are the values a platform accessibility bus carries, and a parallel copy would convert on every
//! property of every node while adding nothing.

/// Which way a control is laid out, for controls where that changes how it is operated.
///
/// A vertical slider is incremented by the up arrow and a horizontal one by the right arrow, so a
/// control whose orientation is not the default for its role has to say so.
pub use accesskit::Orientation;

/// Which way the text of an element runs.
pub use accesskit::TextDirection;

/// Whether a control's value is rejected, and if so on what grounds.
pub use accesskit::Invalid;

/// The three-valued checked state: on, off, or mixed.
///
/// Mixed is not a decoration — a checkbox summarising a partially selected group is mixed, and a
/// consumer announces it differently from either of the other two.
pub use accesskit::Toggled;

/// Which way a sortable column is currently sorted.
pub use accesskit::SortDirection;

/// Which element of a set is the current one, and in what sense.
///
/// A step in a wizard, a page in a pager and a link to the page being viewed are all "current" in
/// different senses, and the sense is what a consumer announces.
pub use accesskit::AriaCurrent;

/// What kind of completion a text field offers.
pub use accesskit::AutoComplete;

/// How urgently a change to a region should interrupt what is being announced.
pub use accesskit::Live;

/// What kind of surface a control opens.
pub use accesskit::HasPopup;

/// The identity of a node in an accessibility tree.
///
/// It is the same integer the framework identifies the node by, so relating one node to another
/// needs no mapping table and no identity that can go stale.
pub use accesskit::NodeId;
