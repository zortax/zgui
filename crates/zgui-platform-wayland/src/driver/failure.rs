//! The failures that end the connection to the compositor, and how the loop reports them.
//!
//! A protocol error and a lost connection are both final: the compositor closes the socket, and
//! every later request and read fails the same way. The connection keeps the first such failure.
//! The loop asks for it after every dispatch and stops at once, so the run ends with that error and
//! the process can be started again.

use std::io::ErrorKind;

use wayland_backend::client::WaylandError;
use zgui_platform::PlatformError;

use crate::driver::WaylandState;

/// Whether `error` ends the connection.
///
/// A full socket buffer is the one failure a later flush recovers from.
pub(crate) fn is_fatal(error: &WaylandError) -> bool {
    match error {
        WaylandError::Io(io) => io.kind() != ErrorKind::WouldBlock,
        WaylandError::Protocol(_) => true,
    }
}

/// The error a run ends with, in the compositor's words where it left any.
fn describe(error: &WaylandError) -> PlatformError {
    match error {
        WaylandError::Protocol(protocol) => {
            let message = if protocol.message.is_empty() {
                String::new()
            } else {
                format!(": {}", protocol.message)
            };
            PlatformError::Backend(format!(
                "the compositor refused a request on {}#{} (error {}){message}",
                protocol.object_interface, protocol.object_id, protocol.code
            ))
        }
        WaylandError::Io(io) => {
            PlatformError::Backend(format!("the connection to the compositor was lost: {io}"))
        }
    }
}

impl WaylandState {
    /// The fatal failure the connection holds, when it holds one.
    ///
    /// A flush is what asks: the connection answers it with the failure it keeps, and a flush
    /// with nothing queued costs no system call.
    pub(crate) fn connection_failure(&self) -> Option<PlatformError> {
        match self.conn.flush() {
            Err(error) if is_fatal(&error) => Some(self.explain_wayland(&error)),
            _ => None,
        }
    }

    /// Why the loop stopped, in the compositor's words where it left any.
    ///
    /// A loop that fails reports an input-output error, because by the time the socket breaks the
    /// reason has already been sent down it. The connection keeps that reason: the interface, the
    /// object and the code of the rule that was broken, or the loss of the socket.
    pub(crate) fn explain(&self, error: &calloop::Error) -> PlatformError {
        self.connection_failure()
            .unwrap_or_else(|| PlatformError::Backend(format!("the wayland loop: {error}")))
    }

    /// The same, for a failure the connection reported itself.
    fn explain_wayland(&self, error: &WaylandError) -> PlatformError {
        match self.conn.protocol_error() {
            Some(protocol) => describe(&WaylandError::Protocol(protocol)),
            None => describe(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use wayland_backend::client::WaylandError;
    use wayland_backend::protocol::ProtocolError;

    use super::{describe, is_fatal};

    fn refused(message: &str) -> WaylandError {
        WaylandError::Protocol(ProtocolError {
            code: 0,
            object_id: 241,
            object_interface: "zwlr_layer_surface_v1".to_owned(),
            message: message.to_owned(),
        })
    }

    #[test]
    fn a_protocol_error_and_a_lost_connection_end_the_run() {
        assert!(is_fatal(&refused("")));
        assert!(is_fatal(&WaylandError::Io(
            io::ErrorKind::BrokenPipe.into()
        )));
        assert!(is_fatal(&WaylandError::Io(
            io::ErrorKind::ConnectionReset.into()
        )));
    }

    #[test]
    fn a_full_socket_buffer_is_retried() {
        assert!(!is_fatal(&WaylandError::Io(
            io::ErrorKind::WouldBlock.into()
        )));
    }

    #[test]
    fn the_error_names_the_refused_object() {
        let text = describe(&refused("wrong configure serial: 93")).to_string();
        assert!(text.contains("zwlr_layer_surface_v1#241"), "{text}");
        assert!(text.contains("wrong configure serial: 93"), "{text}");
        let bare = describe(&refused("")).to_string();
        assert!(bare.ends_with("(error 0)"), "{bare}");
    }
}
