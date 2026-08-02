//! What asking a surface for a texture can say, and what each answer means.
//!
//! Acquisition has seven outcomes, not two, and six of them are ordinary events in a window's
//! life. They are written out here rather than discovered one bug report at a time, and the rule
//! that governs all of them is stated once:
//!
//! > **Damage is retired when the frame's work was submitted, not when a frame was presented.**
//!
//! A frame composes into a target that outlives it, so a frame that drew everything and then
//! failed to acquire a surface has still updated that target. Only a frame that recorded nothing
//! at all — an unconfigured surface — keeps its damage for the next attempt.

use zgui_render::{FrameOutcome, FrameStats, SkipReason};

/// What a request for a surface texture answered.
///
/// The same seven answers wgpu gives, without the texture, so the decision they imply can be
/// written and tested as a plain function.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Acquisition {
    /// A texture was acquired.
    Success,
    /// A texture was acquired, but it no longer matches the surface underneath it.
    Suboptimal,
    /// The compositor did not hand one over in time.
    Timeout,
    /// The window is not visible.
    Occluded,
    /// The surface has changed and the configuration no longer describes it.
    Outdated,
    /// The surface is gone and has to be recreated from the window.
    Lost,
    /// The request itself was rejected.
    Validation,
}

/// What a frame does to the surface once it has answered.
///
/// The three are ordered by how much they cost and are deliberately not combinable: a surface that
/// has to be created again cannot also be reconfigured, because the configuration belongs to the
/// surface that went away.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceAction {
    /// Leave it alone.
    Nothing,
    /// Configure it again before the next frame, which the renderer does for itself.
    Reconfigure,
    /// Mark it as no longer describing the window, so that whatever owns the window makes another.
    Recreate,
}

/// Everything that follows from one acquisition answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Response {
    /// Whether a texture came back and the frame can be copied into it.
    pub presents: bool,
    /// Whether the surface must be configured again before the next frame.
    pub reconfigure: bool,
    /// Whether the surface itself must be recreated from the window handle first.
    pub recreate_surface: bool,
    /// Whether the whole surface must be redrawn next frame.
    ///
    /// Anything that changes the surface underneath a frame invalidates what the composed target
    /// holds relative to it, because nothing observed what the compositor did in between.
    pub force_full_damage: bool,
}

impl Response {
    /// What the frame does to the surface.
    ///
    /// Reconfiguration and recreation are separate outcomes rather than two flags read in turn,
    /// because reading them in turn is how the reconfiguration an outdated surface asks for gets
    /// lost behind the redraw it also asks for — and a surface marked as no longer describing its
    /// window is a surface nothing draws to again until the window produces another one.
    pub const fn surface_action(self) -> SurfaceAction {
        if self.recreate_surface {
            return SurfaceAction::Recreate;
        }
        if self.reconfigure {
            return SurfaceAction::Reconfigure;
        }
        SurfaceAction::Nothing
    }
}

impl Acquisition {
    /// Every answer, which is what makes "all seven are handled" checkable rather than asserted.
    pub const ALL: [Self; 7] = [
        Self::Success,
        Self::Suboptimal,
        Self::Timeout,
        Self::Occluded,
        Self::Outdated,
        Self::Lost,
        Self::Validation,
    ];

    /// The answer's name, which is how it is asked for by hand.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Suboptimal => "suboptimal",
            Self::Timeout => "timeout",
            Self::Occluded => "occluded",
            Self::Outdated => "outdated",
            Self::Lost => "lost",
            Self::Validation => "validation",
        }
    }

    /// Which answer wgpu gave.
    pub fn classify(texture: &wgpu::CurrentSurfaceTexture) -> Self {
        match texture {
            wgpu::CurrentSurfaceTexture::Success(_) => Self::Success,
            wgpu::CurrentSurfaceTexture::Suboptimal(_) => Self::Suboptimal,
            wgpu::CurrentSurfaceTexture::Timeout => Self::Timeout,
            wgpu::CurrentSurfaceTexture::Occluded => Self::Occluded,
            wgpu::CurrentSurfaceTexture::Outdated => Self::Outdated,
            wgpu::CurrentSurfaceTexture::Lost => Self::Lost,
            wgpu::CurrentSurfaceTexture::Validation => Self::Validation,
        }
    }

    /// What to do about it.
    ///
    /// A suboptimal texture is presented and the surface reconfigured afterwards, rather than
    /// dropped: the texture is usable, dropping it drops a frame the user would have seen, and the
    /// condition corrects itself at the next configuration. An outdated one is not retried inside
    /// the same frame, because the work already recorded is sized for the surface that went away.
    pub const fn response(self) -> Response {
        let none = Response {
            presents: false,
            reconfigure: false,
            recreate_surface: false,
            force_full_damage: false,
        };
        match self {
            Self::Success => Response {
                presents: true,
                ..none
            },
            Self::Suboptimal => Response {
                presents: true,
                reconfigure: true,
                ..none
            },
            Self::Timeout | Self::Occluded => none,
            Self::Outdated => Response {
                reconfigure: true,
                force_full_damage: true,
                ..none
            },
            Self::Lost => Response {
                reconfigure: true,
                recreate_surface: true,
                force_full_damage: true,
                ..none
            },
            Self::Validation => none,
        }
    }

    /// The outcome a frame reports for this answer.
    ///
    /// `stats` describes the work that was submitted, which happened whichever answer came back.
    pub const fn outcome(self, stats: FrameStats) -> FrameOutcome {
        match self {
            Self::Success | Self::Suboptimal => FrameOutcome::Presented(stats),
            Self::Timeout => FrameOutcome::Skipped(SkipReason::Timeout),
            Self::Occluded => FrameOutcome::Skipped(SkipReason::Occluded),
            Self::Outdated => FrameOutcome::Skipped(SkipReason::Outdated),
            Self::Lost => FrameOutcome::Recovered,
            Self::Validation => FrameOutcome::Skipped(SkipReason::Validation),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Acquisition, SurfaceAction};
    use zgui_render::{FrameOutcome, FrameStats, SkipReason};

    #[test]
    fn every_arm_has_a_name_of_its_own() {
        let mut names: Vec<&str> = Acquisition::ALL.iter().map(|arm| arm.name()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), Acquisition::ALL.len());
    }

    #[test]
    fn every_arm_retires_its_damage_because_every_arm_submitted_its_work() {
        for arm in Acquisition::ALL {
            assert!(
                arm.outcome(FrameStats::default()).retires_damage(),
                "{arm:?} did not retire its damage, but its frame was submitted"
            );
        }
    }

    #[test]
    fn only_two_arms_reach_the_screen() {
        let presented: Vec<Acquisition> = Acquisition::ALL
            .into_iter()
            .filter(|arm| arm.response().presents)
            .collect();
        assert_eq!(
            presented,
            vec![Acquisition::Success, Acquisition::Suboptimal]
        );
    }

    #[test]
    fn a_suboptimal_texture_is_presented_and_the_surface_reconfigured_afterwards() {
        let response = Acquisition::Suboptimal.response();
        assert!(response.presents, "dropping it would drop a usable frame");
        assert!(response.reconfigure);
        assert!(
            !response.force_full_damage,
            "the frame that was just presented is still what the target holds"
        );
        assert!(matches!(
            Acquisition::Suboptimal.outcome(FrameStats::default()),
            FrameOutcome::Presented(_)
        ));
    }

    #[test]
    fn an_occluded_window_does_not_ask_for_another_frame_and_an_expired_one_does() {
        assert!(
            !Acquisition::Occluded
                .outcome(FrameStats::default())
                .wants_another_frame()
        );
        for arm in [
            Acquisition::Timeout,
            Acquisition::Outdated,
            Acquisition::Validation,
            Acquisition::Lost,
        ] {
            assert!(
                arm.outcome(FrameStats::default()).wants_another_frame(),
                "{arm:?} left the window showing a stale frame with nobody asking for a new one"
            );
        }
    }

    #[test]
    fn the_two_arms_that_change_the_surface_force_a_full_redraw() {
        let forced: Vec<Acquisition> = Acquisition::ALL
            .into_iter()
            .filter(|arm| arm.response().force_full_damage)
            .collect();
        assert_eq!(forced, vec![Acquisition::Outdated, Acquisition::Lost]);
    }

    #[test]
    fn an_outdated_surface_is_configured_again_rather_than_abandoned() {
        // An outdated surface is the ordinary answer to a window being resized underneath a frame,
        // and it asks for both a redraw and a reconfiguration. Marking it as no longer describing
        // its window instead would leave the renderer waiting for a call only the windowing layer
        // can make, which on a resize nothing is obliged to make: a window that appears and never
        // paints again.
        assert_eq!(
            Acquisition::Outdated.response().surface_action(),
            SurfaceAction::Reconfigure
        );
        assert!(Acquisition::Outdated.response().force_full_damage);
    }

    #[test]
    fn every_arm_asking_for_a_reconfiguration_gets_one_unless_the_surface_itself_is_gone() {
        for arm in Acquisition::ALL {
            let response = arm.response();
            let action = response.surface_action();
            if response.reconfigure {
                assert_ne!(
                    action,
                    SurfaceAction::Nothing,
                    "{arm:?} asked to be configured again and nothing acted on it"
                );
            } else {
                assert_eq!(action, SurfaceAction::Nothing, "{arm:?}");
            }
            assert_eq!(
                action == SurfaceAction::Recreate,
                response.recreate_surface,
                "{arm:?}"
            );
        }
    }

    #[test]
    fn only_a_lost_surface_is_recreated_rather_than_reconfigured() {
        for arm in Acquisition::ALL {
            let response = arm.response();
            if response.recreate_surface {
                assert_eq!(arm, Acquisition::Lost);
                assert!(
                    response.reconfigure,
                    "a recreated surface still has to be configured"
                );
            }
        }
    }

    #[test]
    fn each_arm_maps_to_the_outcome_that_names_it() {
        assert_eq!(
            Acquisition::Timeout.outcome(FrameStats::default()),
            FrameOutcome::Skipped(SkipReason::Timeout)
        );
        assert_eq!(
            Acquisition::Outdated.outcome(FrameStats::default()),
            FrameOutcome::Skipped(SkipReason::Outdated)
        );
        assert_eq!(
            Acquisition::Validation.outcome(FrameStats::default()),
            FrameOutcome::Skipped(SkipReason::Validation)
        );
        assert_eq!(
            Acquisition::Lost.outcome(FrameStats::default()),
            FrameOutcome::Recovered
        );
    }
}
