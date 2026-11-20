//! The document identities of the running application, and when one is used again.
//!
//! A node handle carries the identity of the document it was minted in, and the identity space is
//! small: [`DOCUMENT_COUNT`](zgui_view::DOCUMENT_COUNT) at once. A shell opens and closes pop-ups
//! for days, so an identity has to come back once its window is gone. Two rules keep a stale handle
//! from resolving in the document that takes the identity over:
//!
//! - An identity comes back only after its document is dropped. The view layer and the window
//!   host hold the document, so a node reference or a host that outlives its window keeps the
//!   identity out of circulation.
//! - The successor starts each node slot past the last counter the old document issued for it, so
//!   a raw handle of the old document fails the generation check in the new one.
//!
//! Fresh identities are handed out first. After that, the identity that closed first comes back
//! first. An identity whose document-node slot ran out of counters is spent and never comes back.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::{Rc, Weak};

use zgui_dom::{Document, DocumentId, Succession};

use crate::error::AppError;

/// A closed window's document, waiting until its identity can be used again.
struct Closed {
    /// The identity.
    id: DocumentId,
    /// The document, alive while anything still holds it.
    document: Weak<RefCell<Document>>,
    /// The node counters the document hands on.
    succession: Succession,
}

/// The document identities, fresh and given back.
#[derive(Default)]
pub(crate) struct Documents {
    /// The next identity never handed out.
    next: u16,
    /// The identities of closed windows, in the order they closed.
    closed: VecDeque<Closed>,
}

impl Documents {
    /// A document under a fresh identity, or under one a closed window gave back.
    ///
    /// # Errors
    ///
    /// [`AppError::DocumentsExhausted`] when every identity belongs to an open window or to a
    /// document that something still holds.
    pub(crate) fn allocate(&mut self) -> Result<Document, AppError> {
        if let Some(id) = DocumentId::new(self.next) {
            self.next += 1;
            return Ok(Document::with_id(id));
        }
        let mut index = 0;
        while index < self.closed.len() {
            if self.closed[index].document.strong_count() > 0 {
                index += 1;
                continue;
            }
            let closed = self.closed.remove(index).expect("the index is in range");
            if let Some(document) = Document::succeeding(closed.id, &closed.succession) {
                return Ok(document);
            }
            tracing::debug!(
                target: "zgui::app",
                id = closed.id.get(),
                "a document identity is spent and is not used again"
            );
        }
        Err(AppError::DocumentsExhausted)
    }

    /// Gives the identity of `document` back, for use once the document is dropped.
    ///
    /// Called when its window closes. The window's view is unmounted by then, so the document mints
    /// no node after this and the counters taken here are the last it issues.
    pub(crate) fn give_back(&mut self, document: &Rc<RefCell<Document>>) {
        let borrowed = document.borrow();
        self.closed.push_back(Closed {
            id: borrowed.store().document(),
            document: Rc::downgrade(document),
            succession: borrowed.store().succession(),
        });
    }

    /// How many given-back identities wait for their documents to be dropped or for reuse.
    #[cfg(test)]
    fn waiting(&self) -> usize {
        self.closed.len()
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use zgui_view::DOCUMENT_COUNT;

    use super::Documents;
    use crate::error::AppError;

    /// Hands out every fresh identity, holding each document.
    fn exhaust(documents: &mut Documents) -> Vec<Rc<RefCell<zgui_dom::Document>>> {
        (0..DOCUMENT_COUNT)
            .map(|_| Rc::new(RefCell::new(documents.allocate().expect("fresh"))))
            .collect()
    }

    #[test]
    fn an_identity_comes_back_only_once_its_document_is_dropped() {
        let mut documents = Documents::default();
        let mut open = exhaust(&mut documents);
        assert!(matches!(
            documents.allocate(),
            Err(AppError::DocumentsExhausted)
        ));

        let closed = open.remove(7);
        let id = closed.borrow().store().document();
        let stale = closed
            .borrow()
            .store()
            .key_of(closed.borrow().document_index());
        documents.give_back(&closed);
        let holder = Rc::clone(&closed);
        drop(closed);
        assert!(
            matches!(documents.allocate(), Err(AppError::DocumentsExhausted)),
            "something still holds the document"
        );

        drop(holder);
        let reused = documents.allocate().expect("the document is gone");
        assert_eq!(reused.store().document(), id);
        assert_eq!(reused.store().index_of(stale), None);
        assert_eq!(documents.waiting(), 0);
    }

    #[test]
    fn the_identity_that_closed_first_comes_back_first() {
        let mut documents = Documents::default();
        let mut open = exhaust(&mut documents);
        let second = open.remove(20);
        let first = open.remove(10);
        let first_id = first.borrow().store().document();
        documents.give_back(&first);
        documents.give_back(&second);
        drop((first, second));
        let reused = documents.allocate().expect("both are gone");
        assert_eq!(reused.store().document(), first_id);
    }
}
