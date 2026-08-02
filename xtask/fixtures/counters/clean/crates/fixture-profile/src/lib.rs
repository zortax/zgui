//! The fixture's counter set.

counters! {
    /// A counter the stage below increments.
    Alpha => alpha, Group::BackendNeutral;

    /// A counter the clean fixture increments and the planted one does not.
    Beta => beta, Group::BackendNeutral;
}

#[cfg(test)]
mod tests {
    /// A counter declared inside a test module is not the crate's counter set.
    counters! {
        Ignored => ignored, Group::BackendNeutral;
    }
}
