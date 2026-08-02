//! The fixture's counter set.

counters! {
    /// A counter the stage below increments.
    Alpha => alpha, Group::BackendNeutral;

    /// A counter the clean fixture increments and the planted one does not.
    Beta => beta, Group::BackendNeutral;

    /// A counter the stage below increments while the checker still holds a promise to wire it.
    ///
    /// The name is one of the real workspace's, because the list of counters awaiting a stage
    /// belongs to the checker rather than to the tree it is reading. Incrementing it here is the
    /// second violation the check exists to catch: a promise that has already been kept and never
    /// retired, which leaves the list saying nothing about the counters still on it.
    DrawCalls => draw_calls, Group::RendererSpecific;
}

#[cfg(test)]
mod tests {
    /// A counter declared inside a test module is not the crate's counter set.
    counters! {
        Ignored => ignored, Group::BackendNeutral;
    }
}
