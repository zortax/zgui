//! One situation a skip counter is driven through.

/// A named piece of work, with the description that ends up in the failure message.
///
/// The description is not decoration. A skip assertion fails by saying that a counter did not move
/// where it should have, and the only thing that makes that sentence actionable is knowing which
/// situation was being driven — so the description is required rather than derived from a variable
/// name nobody outside the test can see.
pub struct Scenario<'a> {
    /// How the situation reads in a failure message.
    pub(crate) described: &'a str,
    /// The code that puts the process into it.
    drive: Box<dyn FnMut() + 'a>,
}

impl<'a> Scenario<'a> {
    /// A scenario described by `described` and driven by `drive`.
    ///
    /// ```
    /// use zgui_profile::counter::non_vacuity::Scenario;
    ///
    /// let mut ran = false;
    /// let scenario = Scenario::new("a document that has already been painted", || ran = true);
    /// assert_eq!(scenario.described(), "a document that has already been painted");
    /// ```
    pub fn new(described: &'a str, drive: impl FnMut() + 'a) -> Self {
        Self {
            described,
            drive: Box::new(drive),
        }
    }

    /// How the situation reads in a failure message.
    pub fn described(&self) -> &'a str {
        self.described
    }

    /// Puts the process into the situation.
    pub(crate) fn drive(&mut self) {
        (self.drive)();
    }
}

#[cfg(test)]
mod tests {
    use super::Scenario;

    #[test]
    fn a_scenario_runs_its_body_when_it_is_driven_and_not_before() {
        let mut times = 0;
        {
            let mut scenario = Scenario::new("counting", || times += 1);
            scenario.drive();
            scenario.drive();
        }
        assert_eq!(times, 2);
    }
}
