//! Acquiring something to copy a finished frame into, and putting it on the screen.

use crate::frame::fault::FaultInjector;
use crate::target::swapchain::{Presentation, Presented};

/// Asks for something to copy this frame into, letting an injector answer instead.
///
/// Acquisition happens after every draw is recorded and immediately before the copy, so a failed
/// acquisition costs only the copy — and the frame's work is submitted either way, which is why
/// its damage is retired either way. That rule inverts the obvious one and is what makes every
/// unhappy answer safe: the target the frame composed into is persistent, so a frame that drew
/// everything and then failed to acquire has still updated it.
///
/// An injected answer that would have presented still acquires, because a presented frame has to
/// have somewhere to go. An injected answer that would not present skips the acquisition
/// altogether, which is what makes a `Lost` or an `Outdated` reproducible on a surface that is
/// perfectly healthy.
pub fn acquire(presentation: &Presentation, injector: &mut FaultInjector) -> Presented {
    let Some(injected) = injector.take() else {
        return presentation.acquire();
    };
    tracing::debug!(answer = injected.name(), "injecting a surface answer");
    if injected.response().presents {
        let mut presented = presentation.acquire();
        if presented.acquisition.response().presents {
            presented.acquisition = injected;
        }
        return presented;
    }
    Presented {
        acquisition: injected,
        surface_texture: None,
        view: None,
    }
}
