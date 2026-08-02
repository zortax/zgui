//! Rewriting a sheet's text without moving it in the cascade.
//!
//! Replacement always happens, even when the new text has errors. There is no failure state in
//! which the old sheet survives: the parser drops what it cannot use and installs the rest, so a
//! sheet saved with a typo in one declaration applies every other rule in it and reports the one
//! that went. A replacement that refused the whole file on any complaint would mean a single
//! unknown property deleted an application's styling.
//!
//! What replacement buys over removing and adding is the position: the sheets at one origin
//! cascade in installation order, so a sheet re-added at the end would start winning against every
//! sheet that used to beat it.

use style::shared_lock::SharedRwLock;
use style::stylist::Stylist;
use zgui_dom::SheetLoader;

use crate::sheets::errors::CssDiagnostics;
use crate::sheets::set::SheetHandle;
use crate::sheets::{SheetSource, Sheets};

impl Sheets {
    /// Replaces the text of the sheet `handle` names, keeping its place in the cascade.
    ///
    /// # Panics
    ///
    /// Panics if `handle` names no sheet installed here.
    pub(crate) fn replace(
        &mut self,
        stylist: &mut Stylist,
        lock: &SharedRwLock,
        loader: &dyn SheetLoader,
        handle: &SheetHandle,
        source: SheetSource<'_>,
    ) -> CssDiagnostics {
        let origin = self
            .set
            .origin_of(handle.id())
            .expect("the handle names a sheet installed in this rule set");
        let sheet = self.parse(lock, loader, origin, source);
        let guard = lock.read();
        self.set.replace(stylist, &guard, handle, sheet);
        drop(guard);
        self.sink.take()
    }
}
