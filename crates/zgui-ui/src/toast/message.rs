//! What one announcement says, and what kind of thing it is announcing.

use core::fmt;
use core::time::Duration;
use std::rc::Rc;

use zgui_ui_icons::IconData;
use zgui_ui_icons::set::status::{ALERT_TRIANGLE, CHECK_CIRCLE, CROSS_CIRCLE, INFO};
use zgui_ui_icons::set::ui::SPINNER;

/// What kind of thing a toast is announcing.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub enum ToastKind {
    /// Something happened, and nothing is wrong.
    #[default]
    Normal,
    /// Something the user asked for worked.
    Success,
    /// Something the user should know, before it becomes a problem.
    Info,
    /// Something the user should know before it becomes a problem.
    Warning,
    /// Something went wrong.
    Error,
    /// Something is still happening.
    Loading,
}

impl ToastKind {
    /// How this is written as an attribute value, which is what a style sheet selects on.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Success => "success",
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
            Self::Loading => "loading",
        }
    }

    /// The mark drawn before the title, for the kinds that have one.
    ///
    /// Nothing for an ordinary message: a mark beside every announcement is a mark that means
    /// nothing, and the ones that do mean something are then harder to see.
    #[must_use]
    pub const fn mark(self) -> Option<IconData> {
        match self {
            Self::Normal => None,
            Self::Success => Some(CHECK_CIRCLE),
            Self::Info => Some(INFO),
            Self::Warning => Some(ALERT_TRIANGLE),
            Self::Error => Some(CROSS_CIRCLE),
            Self::Loading => Some(SPINNER),
        }
    }

    /// How urgently a toast of this kind should interrupt a reader.
    ///
    /// Errors interrupt; everything else waits for a pause. A library that announced every
    /// "Copied" assertively would be a library people turn their screen reader off for.
    #[must_use]
    pub const fn live(self) -> zgui::vocab::Live {
        match self {
            Self::Error => zgui::vocab::Live::Assertive,
            _ => zgui::vocab::Live::Polite,
        }
    }
}

/// A button on a toast: what it says, and what pressing it does.
///
/// The work is held as a shared closure rather than as a callback prop, because the button belongs
/// to the message and the message travels: it is written where something happened and read where the
/// stack is drawn, which may be four components away.
#[derive(Clone)]
pub struct ToastAction {
    /// What the button says.
    label: String,
    /// What pressing it does.
    run: Rc<dyn Fn()>,
}

impl ToastAction {
    /// A button called `label` that runs `run` when it is pressed.
    #[must_use]
    pub fn new(label: impl Into<String>, run: impl Fn() + 'static) -> Self {
        Self {
            label: label.into(),
            run: Rc::new(run),
        }
    }

    /// What the button says.
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Does what pressing it does.
    pub fn run(&self) {
        (self.run)();
    }
}

/// Two actions are the same one when they say the same thing and do the same thing, which for a
/// closure means being the same closure. A message is compared to decide whether the stack changed,
/// and there is no other answer a function has.
impl PartialEq for ToastAction {
    fn eq(&self, other: &Self) -> bool {
        self.label == other.label && Rc::ptr_eq(&self.run, &other.run)
    }
}

impl Eq for ToastAction {}

impl fmt::Debug for ToastAction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ToastAction")
            .field("label", &self.label)
            .finish_non_exhaustive()
    }
}

/// One message waiting to be read.
///
/// ```
/// use core::time::Duration;
/// use zgui_ui::toast::{Toast, ToastKind};
///
/// let saved = Toast::new("Saved")
///     .description("Your changes are on the server.")
///     .kind(ToastKind::Success)
///     .duration(Duration::from_secs(3));
/// assert_eq!(saved.title(), "Saved");
/// ```
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Toast {
    /// The line a reader hears first.
    title: String,
    /// The line under it, when there is one.
    description: Option<String>,
    /// What kind of thing it is announcing.
    kind: ToastKind,
    /// How long it stays, or `None` for one that waits to be dismissed.
    duration: Option<Duration>,
    /// The button that does the thing the message is about, when there is one.
    action: Option<ToastAction>,
    /// The button that puts the message away without doing anything, when there is one.
    cancel: Option<ToastAction>,
}

impl Toast {
    /// How long a toast stays when nothing says otherwise.
    pub const DEFAULT_DURATION: Duration = Duration::from_secs(4);

    /// A toast with a title and nothing else.
    #[must_use]
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            description: None,
            kind: ToastKind::Normal,
            duration: Some(Self::DEFAULT_DURATION),
            action: None,
            cancel: None,
        }
    }

    /// Adds the line under the title.
    #[must_use]
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Says what kind of thing it is announcing.
    #[must_use]
    pub fn kind(mut self, kind: ToastKind) -> Self {
        self.kind = kind;
        self
    }

    /// Says how long it stays.
    #[must_use]
    pub fn duration(mut self, duration: Duration) -> Self {
        self.duration = Some(duration);
        self
    }

    /// Makes it stay until it is dismissed.
    #[must_use]
    pub fn persistent(mut self) -> Self {
        self.duration = None;
        self
    }

    /// Adds the button that does the thing the message is about.
    #[must_use]
    pub fn action(mut self, label: impl Into<String>, run: impl Fn() + 'static) -> Self {
        self.action = Some(ToastAction::new(label, run));
        self
    }

    /// Adds the button that puts the message away without doing anything.
    #[must_use]
    pub fn cancel(mut self, label: impl Into<String>, run: impl Fn() + 'static) -> Self {
        self.cancel = Some(ToastAction::new(label, run));
        self
    }

    /// The button that does the thing the message is about, when there is one.
    #[must_use]
    pub const fn action_button(&self) -> Option<&ToastAction> {
        self.action.as_ref()
    }

    /// The button that puts the message away, when there is one.
    #[must_use]
    pub const fn cancel_button(&self) -> Option<&ToastAction> {
        self.cancel.as_ref()
    }

    /// The line a reader hears first.
    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    /// The line under it, when there is one.
    #[must_use]
    pub fn body(&self) -> Option<&str> {
        self.description.as_deref()
    }

    /// What kind of thing it is announcing.
    #[must_use]
    pub const fn what(&self) -> ToastKind {
        self.kind
    }

    /// How long it stays, or `None` for one that waits to be dismissed.
    #[must_use]
    pub const fn stays_for(&self) -> Option<Duration> {
        self.duration
    }
}

#[cfg(test)]
mod tests {
    use super::{Toast, ToastKind};

    #[test]
    fn only_an_error_interrupts() {
        // The defect this prevents is a library that announces "Copied" assertively, which is the
        // fastest way to make somebody turn their screen reader off.
        assert_eq!(ToastKind::Error.live(), zgui::vocab::Live::Assertive);
        for kind in [ToastKind::Normal, ToastKind::Success, ToastKind::Warning] {
            assert_eq!(kind.live(), zgui::vocab::Live::Polite);
        }
    }

    #[test]
    fn a_persistent_toast_has_no_deadline_at_all() {
        assert!(Toast::new("Uploading").persistent().stays_for().is_none());
        assert_eq!(
            Toast::new("Saved").stays_for(),
            Some(Toast::DEFAULT_DURATION)
        );
    }
}
