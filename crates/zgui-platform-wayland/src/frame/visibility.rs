//! Whether the compositor is still drawing this surface.

/// How many frames may be given up on before the compositor is assumed not to be drawing at all.
///
/// One is an accident: a compositor under load misses a callback, a monitor changes mode, a
/// workspace animates. A run of them is not — a compositor that has answered for nothing over
/// several frames of its own output has stopped compositing this surface, whatever it did or did
/// not say about it.
///
/// Three, because the cost of being wrong is asymmetric. Reporting a visible window as hidden
/// stops its animations, which the next callback undoes within a frame; reporting a hidden one as
/// visible runs the whole pipeline against pixels nobody can see, for as long as it stays hidden.
const ABANDONED_ENOUGH: u32 = 3;

/// Everything the compositor has said about whether this surface reaches a screen.
///
/// Four signals, and they are not equivalent.
///
/// The compositor **states suspension** outright, and that is the answer whenever it arrives. It
/// arrived in version 6 of the shell, and a client bound below that version never sees it however
/// new the compositor is — which is most clients today, including this one.
///
/// **Leaving every output** is corroboration and nothing more: a surface that has never been on
/// one has not left it, and treating "no outputs yet" as hidden would hold back the very first
/// frame — which is the frame that maps the surface and gets it put on an output in the first
/// place. Several compositors also never send the leave at all for a window merely moved out of
/// sight.
///
/// **The compositor going quiet** is the one that always works. A frame callback that is asked for
/// and never answered, several times over, means the compositor is not drawing this surface — and
/// unlike the other two it needs no protocol version, no optional extension and no cooperation.
/// It is inferred rather than stated, which is why it takes a run rather than a single miss.
///
/// **Being unconfigured** is the fourth, and it is the surface not existing yet rather than being
/// hidden.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Visibility {
    /// Whether the surface has been configured and may be drawn into at all.
    pub configured: bool,
    /// Whether the compositor said it has stopped repainting the surface.
    pub suspended: bool,
    /// How many outputs the surface is currently on.
    pub outputs: usize,
    /// Whether the surface has ever been on an output.
    ///
    /// This is what makes the output count usable. Before the first enter it says nothing, and a
    /// compositor that never sends one leaves the whole signal inert rather than wrong.
    pub entered: bool,
    /// How many frames in a row the compositor has been asked for and not answered.
    pub abandoned: u32,
}

impl Visibility {
    /// Whether the surface is worth drawing.
    pub const fn is_hidden(&self) -> bool {
        if !self.configured {
            return true;
        }
        self.suspended || (self.entered && self.outputs == 0) || self.abandoned >= ABANDONED_ENOUGH
    }

    /// Records the compositor answering for a frame, whether by drawing it or by discarding it.
    ///
    /// Either way it spoke about this surface, which is the whole of what the run counts.
    pub const fn answered(&mut self) {
        self.abandoned = 0;
    }

    /// Records a frame the compositor was asked for and never answered.
    pub const fn unanswered(&mut self) {
        self.abandoned = self.abandoned.saturating_add(1);
    }

    /// Records the surface entering an output.
    pub const fn entered_output(&mut self) {
        self.outputs += 1;
        self.entered = true;
    }

    /// Records the surface leaving an output.
    pub const fn left_output(&mut self) {
        self.outputs = self.outputs.saturating_sub(1);
    }
}

#[cfg(test)]
mod tests {
    use super::Visibility;

    fn shown() -> Visibility {
        Visibility {
            configured: true,
            ..Visibility::default()
        }
    }

    #[test]
    fn a_surface_that_has_not_been_configured_is_not_drawn() {
        assert!(Visibility::default().is_hidden());
    }

    #[test]
    fn a_configured_surface_that_has_entered_no_output_yet_is_drawn() {
        // The first frame is what maps the surface, and a compositor puts a surface on an output
        // only once it is mapped. Withholding it here is a deadlock rather than a saving.
        assert!(!shown().is_hidden());
    }

    #[test]
    fn suspension_hides_the_surface_whatever_the_outputs_say() {
        let mut visibility = shown();
        visibility.entered_output();
        visibility.suspended = true;
        assert!(visibility.is_hidden());
    }

    #[test]
    fn leaving_every_output_it_had_hides_the_surface_and_returning_shows_it() {
        let mut visibility = shown();
        visibility.entered_output();
        assert!(!visibility.is_hidden());
        visibility.left_output();
        assert!(visibility.is_hidden());
        visibility.entered_output();
        assert!(!visibility.is_hidden());
    }

    #[test]
    fn a_compositor_that_goes_quiet_hides_the_surface_without_having_said_anything() {
        // The signal that needs no protocol version and no cooperation. A client bound below
        // version six of the shell never hears about suspension however new the compositor is,
        // and several compositors never send a leave for a window merely moved out of sight.
        let mut visibility = shown();
        visibility.unanswered();
        visibility.unanswered();
        assert!(!visibility.is_hidden(), "one missed frame is an accident");
        visibility.unanswered();
        assert!(visibility.is_hidden());
    }

    #[test]
    fn the_compositor_speaking_again_shows_the_surface_at_once() {
        // Within one frame, because the cost of having been wrong in this direction is a stopped
        // animation and the cost of waiting is a window that stays stopped after it came back.
        let mut visibility = shown();
        for _ in 0..10 {
            visibility.unanswered();
        }
        assert!(visibility.is_hidden());
        visibility.answered();
        assert!(!visibility.is_hidden());
    }

    #[test]
    fn a_run_of_missed_frames_does_not_wrap_however_long_the_window_stays_hidden() {
        let mut visibility = shown();
        visibility.abandoned = u32::MAX;
        visibility.unanswered();
        assert!(visibility.is_hidden());
    }

    #[test]
    fn leaving_more_outputs_than_were_entered_does_not_wrap() {
        // A compositor is free to repeat a leave, and a count that wrapped would report the
        // surface as being on four billion outputs and never hide it again.
        let mut visibility = shown();
        visibility.left_output();
        visibility.left_output();
        assert_eq!(visibility.outputs, 0);
    }
}
