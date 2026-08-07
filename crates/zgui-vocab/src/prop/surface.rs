//! The property a `surface` element's producer is carried in.

/// The property naming which registered producer fills a surface element.
///
/// The value is an integer token into the embed registry — a *name*, exactly as a canvas's
/// property is: the texture, the events and the lifecycle all live on the other side of the
/// registry, because a device resource can no more cross the document than a shape list can.
///
/// Writing it owes no invalidation of its own. The element is replaced by its *tag*, and pixels
/// arriving, resizing or going away are the embed host's to damage — the host is the only party
/// that knows when the texture actually changed, and a repaint on token-write alone would redraw
/// a box whose content has not been produced yet.
pub const SOURCE: &str = "surface-source";
