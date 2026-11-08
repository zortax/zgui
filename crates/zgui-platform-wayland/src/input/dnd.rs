//! Content dragged over a window from another application.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use smithay_client_toolkit::data_device_manager::data_offer::DragOffer;
use zgui_geom::{Css, CssPx, Point};
use zgui_platform::{DragEvent, SurfaceId};

/// The drag that is currently over a window, if there is one.
///
/// # Why the paths are read before the drop
///
/// The contract carries the whole set of paths at every stage, including the one where the pointer
/// is still moving, so that a drop target can decide whether it *would* accept before the user
/// lets go — which is what makes a highlight appear while the drag is happening rather than after.
///
/// The protocol does not hand the paths over: it names the media types on offer and leaves the
/// content to be asked for through a pipe. So the read starts when the drag arrives, and the
/// `Entered` event is produced when it finishes. A drag whose read has not finished by the time
/// the user drops is still delivered — the drop waits for the same read rather than arriving with
/// nothing.
#[derive(Debug, Default)]
pub struct Drag {
    /// The session, while one is over a window.
    session: Option<Session>,
    /// Where a completed read leaves its answer.
    read: Arc<Mutex<Option<Vec<PathBuf>>>>,
}

/// One drag over one window.
#[derive(Debug)]
struct Session {
    /// The window it is over.
    surface: SurfaceId,
    /// The offer the content is asked for through.
    offer: DragOffer,
    /// Where the pointer is.
    at: Point<CssPx, Css>,
    /// Whether the paths have been reported yet.
    announced: bool,
    /// Whether the user has let go and the drop is waiting on the read.
    dropped: bool,
}

impl Drag {
    /// Records a drag arriving over `surface`.
    pub fn entered(&mut self, surface: SurfaceId, offer: DragOffer, at: Point<CssPx, Css>) {
        // A cancelled drag's worker can still finish. Give each session its own mailbox
        // so that an old read cannot supply paths to the next drop.
        self.read = Arc::default();
        self.session = Some(Session {
            surface,
            offer,
            at,
            announced: false,
            dropped: false,
        });
    }

    /// The offer to read the paths out of, when one has just arrived.
    pub fn to_read(&self) -> Option<&DragOffer> {
        let session = self.session.as_ref()?;
        (!session.announced).then_some(&session.offer)
    }

    /// Where a completed read leaves its answer.
    pub fn answers(&self) -> Arc<Mutex<Option<Vec<PathBuf>>>> {
        Arc::clone(&self.read)
    }

    /// Records the pointer moving.
    pub fn moved(&mut self, at: Point<CssPx, Css>) -> Option<(SurfaceId, DragEvent)> {
        let session = self.session.as_mut()?;
        session.at = at;
        session
            .announced
            .then_some((session.surface, DragEvent::Moved { position: at }))
    }

    /// Records the drag leaving without a drop.
    pub fn left(&mut self) -> Option<(SurfaceId, DragEvent)> {
        // wl_data_device.leave also follows a successful drop. Keep the offer until
        // its asynchronous transfer finishes instead of discarding a pending drop.
        if self.session.as_ref().is_some_and(|session| session.dropped) {
            return None;
        }
        let session = self.session.take()?;
        session
            .announced
            .then_some((session.surface, DragEvent::Left))
    }

    /// Records the user letting go.
    ///
    /// Answers with the drop when the paths are already known, and with nothing when they are not
    /// — in which case the read that is still running produces it.
    pub fn dropped(&mut self) -> Option<(SurfaceId, DragEvent)> {
        let session = self.session.as_mut()?;
        session.dropped = true;
        let paths = self
            .read
            .lock()
            .unwrap_or_else(|held| held.into_inner())
            .clone()?;
        Some(self.finish(paths))
    }

    /// Records a read finishing, and says what that means now.
    ///
    /// Before the drop it is the arrival; after it, it is the drop itself. Either way it is the
    /// first moment the paths are known, which is the only moment either event can be produced.
    pub fn read_finished(&mut self, paths: Vec<PathBuf>) -> Option<(SurfaceId, DragEvent)> {
        *self.read.lock().unwrap_or_else(|held| held.into_inner()) = Some(paths.clone());
        let session = self.session.as_mut()?;
        if session.dropped {
            return Some(self.finish(paths));
        }
        if session.announced {
            return None;
        }
        session.announced = true;
        Some((
            session.surface,
            DragEvent::Entered {
                paths,
                position: session.at,
            },
        ))
    }

    /// Completes the drop and takes the session down.
    fn finish(&mut self, paths: Vec<PathBuf>) -> (SurfaceId, DragEvent) {
        let session = self
            .session
            .take()
            .expect("a drop is only finished while a session exists");
        // The offer is told the transfer worked and then retired. A drop that is never finished
        // leaves the source waiting, which on some desktops leaves the dragged icon on screen.
        session.offer.finish();
        session.offer.destroy();
        (
            session.surface,
            DragEvent::Dropped {
                paths,
                position: session.at,
            },
        )
    }
}

/// The paths a list of file addresses names.
///
/// The format is one address per line with comment lines, and only `file` addresses are paths at
/// all — a drag of a web address carries something no file system can open, and turning it into a
/// path would produce a name nothing can read rather than nothing.
pub fn paths(bytes: &[u8]) -> Vec<PathBuf> {
    use std::os::unix::ffi::OsStringExt;
    String::from_utf8_lossy(bytes)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| line.strip_prefix("file://"))
        .filter_map(|address| {
            let path = if address.starts_with('/') {
                address
            } else {
                // Localhost is local; other authorities must not become relative paths.
                address
                    .strip_prefix("localhost")
                    .filter(|path| path.starts_with('/'))?
            };
            let bytes = unescape(path);
            (!bytes.contains(&0)).then(|| PathBuf::from(std::ffi::OsString::from_vec(bytes)))
        })
        .collect()
}

/// One address with its percent escapes resolved.
fn unescape(address: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(address.len());
    let bytes = address.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let digit = |byte: u8| (byte as char).to_digit(16).map(|value| value as u8);
            if let (Some(high), Some(low)) = (digit(bytes[index + 1]), digit(bytes[index + 2])) {
                out.push(high * 16 + low);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::paths;
    use std::path::PathBuf;

    #[test]
    fn filenames_preserve_utf8_and_unix_bytes_without_panicking_on_malformed_escapes() {
        use std::os::unix::ffi::OsStrExt;
        assert_eq!(
            paths("file:///tmp/flight%20%C3%BC.ulg\nfile://localhost/tmp/日本.ulg\n".as_bytes()),
            vec![
                PathBuf::from("/tmp/flight ü.ulg"),
                PathBuf::from("/tmp/日本.ulg")
            ]
        );
        assert_eq!(
            paths(b"file:///tmp/%FF.ulg")[0].as_os_str().as_bytes(),
            b"/tmp/\xFF.ulg"
        );
        assert_eq!(
            paths("file:///tmp/%é.ulg".as_bytes()),
            vec![PathBuf::from("/tmp/%é.ulg")]
        );
        assert!(paths(b"file://remote/tmp/log.ulg\nfile:///tmp/bad%00.ulg").is_empty());
    }

    #[test]
    fn a_list_with_nothing_on_it_names_no_files() {
        assert!(paths(b"").is_empty());
        assert!(paths(b"# a comment and nothing else\r\n").is_empty());
    }

    #[test]
    fn every_address_on_the_list_becomes_a_path() {
        let list = b"file:///home/a/one.png\r\nfile:///home/a/two.png\r\n";
        assert_eq!(
            paths(list),
            vec![
                PathBuf::from("/home/a/one.png"),
                PathBuf::from("/home/a/two.png")
            ]
        );
    }

    #[test]
    fn something_that_is_not_a_file_is_dropped_rather_than_turned_into_a_name() {
        // A web address made into a path is a name nothing can open, which is worse than nothing.
        let mixed = b"https://example.invalid/a\r\nfile:///home/a/one.png\r\n";
        assert_eq!(paths(mixed), vec![PathBuf::from("/home/a/one.png")]);
    }

    #[test]
    fn an_escaped_name_comes_back_as_the_name_it_stands_for() {
        let escaped = b"file:///home/a/two%20words%21.txt\r\n";
        assert_eq!(
            paths(escaped),
            vec![PathBuf::from("/home/a/two words!.txt")]
        );
    }

    #[test]
    fn an_escape_that_is_not_one_is_left_alone_rather_than_dropping_the_character() {
        let odd = b"file:///home/a/100%.txt\r\n";
        assert_eq!(paths(odd), vec![PathBuf::from("/home/a/100%.txt")]);
    }
}
