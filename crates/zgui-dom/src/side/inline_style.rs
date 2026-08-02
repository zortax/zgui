//! The declarations attached to a single node rather than to a rule.
//!
//! There is exactly one block per node, and everything that gives an element a declaration of its
//! own writes into it: the `style` text an author wrote, one declaration a view's binding owns,
//! one custom property a theme sets. The block is kept **parsed**, so replacing one declaration
//! replaces one entry rather than re-parsing the rest — which is the difference between a
//! per-frame cost and a per-frame allocation storm when a single value is animating.
//!
//! A second block beside this one would hold values that never reach the screen: the style engine
//! reads an element's own declarations through a single hook, so anything not in this block takes
//! part in no cascade at all.
//!
//! The block is shared with the style engine under the document's lock, which is the same lock
//! every stylesheet in the document uses. One lock is not an implementation detail: the engine
//! takes a read guard covering everything it may consult during a restyle, and a declaration block
//! behind a different lock cannot be read under that guard at all.

use servo_arc::Arc as ServoArc;
use style::properties::PropertyDeclarationBlock;
use style::shared_lock::Locked;

/// A parsed block of declarations, shared with the style engine under the document's lock.
pub type StyleBlock = ServoArc<Locked<PropertyDeclarationBlock>>;
