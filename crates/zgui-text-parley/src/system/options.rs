//! How a font system is set up before any face reaches it.

/// Where a [`FontSystem`](crate::FontSystem) is allowed to find faces.
///
/// The two modes differ in exactly one thing, and it is the thing that decides whether a rendered
/// result is reproducible.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Enumeration {
    /// Faces come from the operating system as well as from explicit registration.
    ///
    /// What is installed differs between machines, between distributions and between weeks, so a
    /// measurement taken under this mode is a measurement of the machine as much as of the code.
    /// It is what an application wants and what a test never does.
    System,
    /// Faces come only from explicit registration.
    ///
    /// Nothing is discovered, so the same registrations produce the same faces, the same shaped
    /// advances and the same pixels anywhere. The collection starts with no generic-family
    /// bindings at all under this mode, so registering a family binds it to every generic role
    /// nothing else has claimed — otherwise `font-family: serif`, which is what an unstyled
    /// document resolves to, would find no face at all.
    #[default]
    Registered,
}

impl Enumeration {
    /// Whether the operating system's own faces are enumerated.
    pub fn reads_the_system(self) -> bool {
        matches!(self, Self::System)
    }
}

/// How a [`FontSystem`](crate::FontSystem) is built.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct FontSystemOptions {
    /// Where faces come from.
    pub enumeration: Enumeration,
}

impl FontSystemOptions {
    /// A system that reads nothing it was not given.
    pub const fn registered_only() -> Self {
        Self {
            enumeration: Enumeration::Registered,
        }
    }

    /// A system that enumerates the operating system's faces as well.
    pub const fn with_system_fonts() -> Self {
        Self {
            enumeration: Enumeration::System,
        }
    }
}
