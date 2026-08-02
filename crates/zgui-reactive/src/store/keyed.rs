//! Stores: fine-grained reactivity over a struct, addressed by field and by key.

/// A reactive view of an ordinary struct, where each field is independently tracked.
///
/// Reading one field subscribes to that field alone, so a form with thirty inputs re-runs one
/// binding when one of them changes rather than thirty. Derive it on the struct, then reach into
/// it field by field.
///
/// Stores suit *schema-shaped* state: application state with a known set of fields, and
/// collections addressed by a stable key. They are not a general graph store; the fields'
/// bookkeeping is keyed by path and grows with the paths ever touched.
pub use reactive_stores::Store;

/// Updates a value in place from another of the same shape, touching only the fields that
/// differ.
///
/// The point is the "only what differs": replacing a whole value would wake every field's
/// subscribers, so a refresh that changed one row would re-run everything.
pub use reactive_stores::Patch;

/// A type-erased handle to one field of a store, freed with the owner that created it.
///
/// The type to hold in a component that works on a field without caring which store it came
/// from.
pub use reactive_stores::Field;

/// The reference-counted form of [`Field`].
pub use reactive_stores::ArcField;

/// The trait every store field implements: read it, write it, and know where it sits.
pub use reactive_stores::StoreField;

/// One named field of a store, as reached by the derived accessors.
pub use reactive_stores::Subfield;

/// A keyed collection field, whose entries are addressed by key rather than by position.
///
/// Declare the key on the collection field and address entries with `at_key`. Addressing by
/// *position* is deliberately unavailable: an index is not a stable identity, so inserting or
/// removing an entry shifts every later one, waking every observer after the change and — when
/// the collection shrinks — reading an entry that is no longer there.
pub use reactive_stores::KeyedSubfield;

/// One entry of a keyed collection, as returned by `at_key`.
pub use reactive_stores::AtKeyed;
