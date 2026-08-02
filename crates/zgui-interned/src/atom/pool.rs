//! The process-wide table every interned string is looked up in.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock, PoisonError};

use rustc_hash::FxBuildHasher;

use crate::atom::entry::Entry;

/// The table, keyed by the text of an entry that already exists.
type Table = HashMap<&'static str, &'static Entry, FxBuildHasher>;

/// Returns the one entry for `text`, creating it if this is the first time it has been seen.
///
/// The entry is never freed. That is the whole trade: names are drawn from a vocabulary that is
/// small and fixed in practice — element names, attribute names, class names, custom property
/// names — so never freeing them buys a pointer-sized, cheaply comparable handle for each, and
/// costs a bounded amount of memory that is reached in the first seconds of a run.
pub(crate) fn intern(text: &str) -> &'static Entry {
    static POOL: OnceLock<Mutex<Table>> = OnceLock::new();
    let mut table = POOL
        .get_or_init(|| Mutex::new(Table::default()))
        .lock()
        // A panic while the table was locked cannot have left it inconsistent: the only thing
        // that runs under the lock is a lookup and an insert of an entry that lives forever.
        .unwrap_or_else(PoisonError::into_inner);

    if let Some(entry) = table.get(text) {
        return entry;
    }
    let entry: &'static Entry = Box::leak(Box::new(Entry::new(text)));
    table.insert(entry.text(), entry);
    entry
}
