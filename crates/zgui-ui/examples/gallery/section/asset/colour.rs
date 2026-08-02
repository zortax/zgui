//! A document with a palette of its own.
//!
//! Nothing in it says `currentColor`, so nothing in it takes a colour from the page. The same
//! source drawn on a rose card and on a teal one is the same picture both times, which is what
//! separates an illustration from an icon.

/// A view over water: sky, sun, two hills, a river, a house and its roof.
///
/// Seven fills in six distinct hues, chosen so that a picture rendered in one colour is obvious
/// rather than arguable.
pub(crate) const COTTAGE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64">
  <rect x="0" y="0" width="64" height="64" rx="6" fill="#dbeafe"/>
  <circle cx="49" cy="15" r="7.5" fill="#f59e0b"/>
  <path d="M0 46 L17 27 L33 46 Z" fill="#4ade80"/>
  <path d="M21 47 L41 23 L64 47 Z" fill="#15803d"/>
  <rect x="0" y="46" width="64" height="18" fill="#2563eb"/>
  <rect x="9" y="33" width="14" height="13" fill="#f8fafc"/>
  <path d="M6 33 L16 24 L26 33 Z" fill="#dc2626"/>
  <rect x="13" y="38" width="6" height="8" fill="#78350f"/>
</svg>"##;
