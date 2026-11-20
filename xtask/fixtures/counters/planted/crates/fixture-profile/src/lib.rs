//! The fixture's counter set.

counters! {
    /// A counter the stage below increments.
    Alpha => alpha, Group::BackendNeutral;

    /// A counter the clean fixture increments and the planted one does not.
    Beta => beta, Group::BackendNeutral;

    /// A renderer counter the stage below increments.
    ///
    /// It produces no violation while the checker lists no counter as awaiting a stage.
    DrawCalls => draw_calls, Group::RendererSpecific;
}

#[cfg(test)]
mod tests {
    /// A counter declared inside a test module is not the crate's counter set.
    counters! {
        Ignored => ignored, Group::BackendNeutral;
    }
}
