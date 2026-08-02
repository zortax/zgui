//! Documents with no colour of their own: every paint in them is `currentColor`.
//!
//! An asset written this way takes the colour of whatever it is put inside, so one file is the dark
//! mark on a light card and the light mark on a filled button with nothing between them but a
//! `color` declaration.

/// A star inside a ring, filled and stroked with the inherited colour.
pub(crate) const STAR: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
  <circle cx="12" cy="12" r="10.6" fill="none" stroke="currentColor" stroke-width="1.4"/>
  <path fill="currentColor"
        d="M12 4 L14 9.4 L19.6 9.7 L15.3 13.3 L16.7 18.8 L12 15.7 L7.3 18.8 L8.7 13.3 L4.4 9.7 L10 9.4 Z"/>
</svg>"##;

/// Three marks in a space three times as wide as it is tall.
///
/// Deliberately not square: a drawing is fitted into whatever box it is given uniformly and centred
/// in what is left over, and a square asset cannot show the difference between that and a stretch.
pub(crate) const BANNER: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 16">
  <rect x="1" y="1" width="46" height="14" rx="2" fill="none" stroke="currentColor" stroke-width="1.6"/>
  <path fill="currentColor" d="M5 12.5 L11 4 L17 12.5 Z"/>
  <circle cx="24" cy="8" r="4" fill="currentColor"/>
  <rect x="35" y="4" width="8" height="8" fill="currentColor"/>
</svg>"##;

/// The same three marks in a document of its own that declares a size and an aspect rule.
///
/// The `viewBox` is square while the document is twice as wide, so `preserveAspectRatio` is what
/// decides whether the marks stay round, are stretched across, or are cropped — and that decision
/// is made inside the document, before the element's own box is fitted at all.
pub(crate) fn framed(aspect: &str) -> String {
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="96" height="48" viewBox="0 0 48 48"
             preserveAspectRatio="{aspect}">
  <rect x="1" y="1" width="46" height="46" rx="3" fill="none" stroke="currentColor" stroke-width="2"/>
  <circle cx="24" cy="24" r="13" fill="none" stroke="currentColor" stroke-width="2"/>
  <path fill="currentColor" d="M24 12 L30 24 L24 36 L18 24 Z"/>
</svg>"##
    )
}

/// A ramp seen through a diamond-shaped clip.
///
/// The colours are the document's own — a ramp is not a colour anything can inherit — and the clip
/// is an arbitrary outline rather than a rectangle, so what is drawn is decided by the document and
/// not by the element's box.
pub(crate) const FACET: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64">
  <defs>
    <linearGradient id="ramp" x1="0" y1="0" x2="64" y2="64" gradientUnits="userSpaceOnUse">
      <stop offset="0.25" stop-color="#f43f5e"/>
      <stop offset="0.5" stop-color="#f59e0b"/>
      <stop offset="0.75" stop-color="#2563eb"/>
    </linearGradient>
    <clipPath id="facet">
      <path d="M32 1 L63 32 L32 63 L1 32 Z"/>
    </clipPath>
  </defs>
  <g clip-path="url(#facet)">
    <rect x="0" y="0" width="64" height="64" fill="url(#ramp)"/>
  </g>
</svg>"##;
