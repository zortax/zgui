//! Every headless behaviour, in one import.
//!
//! A component written with `view!` needs both the component and the props type its
//! `#[component]` attribute generated, because the macro names the second to build the first.
//! Importing them in pairs by hand is a paper cut per component, so they are exported together
//! here:
//!
//! ```
//! use zgui_ui_primitives::prelude::*;
//! ```
//!
//! Nothing here is exclusive to it — every name is reachable at its own path too.

pub use crate::collection::{Collection, CollectionItem, ItemId};
pub use crate::dismiss::{
    DismissReason, DismissableLayer, DismissableLayerProps, LayerId, LayerStack,
};
pub use crate::focus::{
    FocusScope, FocusScopeProps, Orientation, RovingContext, RovingFocus, RovingFocusProps,
    RovingItem, use_roving_item, use_roving_item_when,
};
pub use crate::popper::{
    Align, Placement, Popper, PopperOptions, PopperProps, Side, Solution, solve,
};
pub use crate::presence::{Presence, PresenceContext, PresenceProps, PresenceState, use_presence};
pub use crate::state::{Binding, Controllable};
