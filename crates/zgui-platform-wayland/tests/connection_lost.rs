//! A lost connection ends the run with an error.
//!
//! The socket to the compositor shuts while the window draws, as it does when the compositor goes
//! away. The run returns the loss.

#[cfg(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
#[path = "support/mod.rs"]
mod support;

/// Runs the test where the platform under test exists, and does nothing elsewhere.
fn main() {
    #[cfg(any(
        target_os = "linux",
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd"
    ))]
    supported::main();
}

/// The test, built only where the platform under test exists.
#[cfg(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
mod supported {
    use super::support;

    use std::net::Shutdown;
    use std::os::unix::net::UnixStream;

    use wayland_client::Connection;

    /// Shuts the application's socket in both directions.
    ///
    /// A shutdown acts on the socket, so a duplicate of its descriptor shuts the one the loop reads.
    fn lose(conn: &Connection) {
        let fd = conn
            .backend()
            .poll_fd()
            .try_clone_to_owned()
            .expect("the socket descriptor duplicates");
        UnixStream::from(fd)
            .shutdown(Shutdown::Both)
            .expect("the socket shuts");
    }

    pub(super) fn main() {
        support::fatal::assert_run_ends(
            "a lost connection ends the run with an error",
            lose,
            "the connection to the compositor was lost",
        );
    }
}
