//! The style engine's DOM traits, implemented over [`Node`](crate::Node).
//!
//! Seven traits, all on the one handle: the document, the node, the shadow root, the element the
//! cascade sees, the element selector matching sees, and the attribute provider a container query
//! reads through. Nothing here holds state of its own. Every method is a read of the node record or
//! of a table reached through the record's back-pointer, which is what makes the whole surface safe
//! to call from several workers at once.
//!
//! # What runs on a worker thread
//!
//! All of it. The engine hands one element to each worker and each worker walks *outwards* — to the
//! parent for a descendant combinator, back along the element chain for a sibling one, down for a
//! structural pseudo-class — so any method here may be running against an element another worker is
//! standing on. Exactly two of them write: the selector-flag recorder, which writes on the element
//! *and on its parent*, and the bookkeeping-bit setters. Both go through atomics for that reason.
//!
//! # Which methods are reachable on a node that is not an element
//!
//! The traits all sit on one handle, so a method belonging to the element half is nominally
//! callable on a text node. It is not reachable, and that is enforced rather than hoped for:
//! selector matching reaches an element only through the parent link, the element-only sibling
//! chain or the matching context's roots, and every one of those already filters to elements.
//!
//! | Family | On an element | On text, a marker or the document |
//! |---|---|---|
//! | tree navigation | full | reachable, and correct — the chains already skip non-elements |
//! | identity: name, namespace, identifier, classes, attributes | full | unreachable |
//! | state: the state pseudo-classes, links, custom states | full | unreachable |
//! | structural: emptiness, root-ness, first element child | full | unreachable |
//!
//! The two unreachable families carry one debug assertion each, at the head of their module, so a
//! future change that widens the entry points fails loudly instead of matching wrongly. A text node
//! reached deliberately — asking a node whether it is an element and being told no — is the
//! supported path and takes no assertion.
//!
//! # Module map
//!
//! | Module | Contents |
//! |---|---|
//! | [`document`] | the document trait: the shared lock and two policy answers |
//! | [`node`] | node identity, node kind and the plain tree links |
//! | [`shadow_root`] | the shadow-root trait, satisfied and unreachable |
//! | [`element`] | the cascade's view of an element, split by concern |
//! | [`selectors`] | the matcher's view of an element, split by concern |
//! | [`attributes`] | the by-name attribute lookup a container query reads through |
//! | [`data`] | reading the engine's per-element output back off the tree |
//! | [`flags`] | the engine's bookkeeping bits, and the descent flag that is a view |
//! | [`bloom`] | the ancestor hashes selector matching filters candidates with |

pub mod attributes;
pub mod bloom;
pub mod data;
pub mod document;
pub mod element;
pub mod flags;
pub mod node;
pub mod selectors;
pub mod shadow_root;
