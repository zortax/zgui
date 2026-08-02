//! What every part of a sidebar reads to know how the panel stands.

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RwSignal};
use zgui_ui_primitives::Controllable;

use crate::sidebar::shape::{SidebarCollapse, SidebarSide, SidebarVariant};

/// What every part of a sidebar reads to know whether it is open and how it is framed.
///
/// The frame owns the open state; the panel owns its own shape, and writes it here so that the
/// frame's rules — which reach every part — can select on it. `Copy`, so a part stores one without
/// cloning.
#[derive(Copy, Clone)]
pub struct SidebarContext {
    /// Whether the panel is open.
    open: Controllable<bool>,
    /// Which side it is on.
    side: RwSignal<SidebarSide, LocalStorage>,
    /// What folding it away leaves behind.
    collapse: RwSignal<SidebarCollapse, LocalStorage>,
    /// What frame the surface sits in.
    variant: RwSignal<SidebarVariant, LocalStorage>,
    /// The panel's own element, so a trigger can say what it controls.
    panel: NodeRef,
}

impl SidebarContext {
    /// A context over the given open state, in the given shape.
    pub(crate) fn new(
        open: Controllable<bool>,
        side: SidebarSide,
        collapse: SidebarCollapse,
        variant: SidebarVariant,
    ) -> Self {
        Self {
            open,
            side: RwSignal::new_local(side),
            collapse: RwSignal::new_local(collapse),
            variant: RwSignal::new_local(variant),
            panel: NodeRef::new(),
        }
    }

    /// The sidebar the calling scope is inside, when it is inside one.
    #[must_use]
    pub fn current() -> Option<Self> {
        use_local_context::<Self>()
    }

    /// Whether the panel is open.
    #[must_use]
    pub fn is_open(self) -> bool {
        self.open.get()
    }

    /// How the state is written as an attribute value.
    #[must_use]
    pub fn state_name(self) -> &'static str {
        if self.is_open() {
            "expanded"
        } else {
            "collapsed"
        }
    }

    /// Which side the panel is on.
    #[must_use]
    pub fn side(self) -> SidebarSide {
        self.side.get()
    }

    /// What folding it away leaves behind.
    #[must_use]
    pub fn collapse(self) -> SidebarCollapse {
        self.collapse.get()
    }

    /// What frame the surface sits in.
    #[must_use]
    pub fn variant(self) -> SidebarVariant {
        self.variant.get()
    }

    /// The panel's own element.
    #[must_use]
    pub fn panel(self) -> NodeRef {
        self.panel
    }

    /// Whether the panel is folded to icon width rather than open or gone.
    #[must_use]
    pub fn is_icon_only(self) -> bool {
        !self.is_open() && self.collapse() == SidebarCollapse::Icon
    }

    /// Says which side the panel took, which the panel does and nothing else should.
    pub(crate) fn take_side(self, side: SidebarSide) {
        if self.side.get_untracked() != side {
            self.side.set(side);
        }
    }

    /// Says what folding the panel leaves behind, which the panel does and nothing else should.
    pub(crate) fn take_collapse(self, collapse: SidebarCollapse) {
        if self.collapse.get_untracked() != collapse {
            self.collapse.set(collapse);
        }
    }

    /// Says what frame the panel took, which the panel does and nothing else should.
    pub(crate) fn take_variant(self, variant: SidebarVariant) {
        if self.variant.get_untracked() != variant {
            self.variant.set(variant);
        }
    }

    /// Folds the panel away if it was open, and brings it back if it was not.
    pub fn toggle(self) {
        if self.collapse.get_untracked() == SidebarCollapse::None {
            return;
        }
        self.open.toggle();
    }

    /// Opens or folds the panel outright.
    pub fn set_open(self, open: bool) {
        if self.collapse.get_untracked() == SidebarCollapse::None {
            return;
        }
        self.open.set(open);
    }
}
