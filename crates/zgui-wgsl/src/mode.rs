//! What a mode means to a translation unit: which parts it is built from, and its entry points.

use zgui_scene::ShaderMode;

/// The parts that follow an application's own text, for `mode`.
pub fn epilogue(mode: ShaderMode) -> &'static [&'static str] {
    match mode {
        ShaderMode::Paint => &["shaded", "shaded_paint"],
        ShaderMode::Coverage => &["shaded", "shaded_coverage"],
        ShaderMode::Filter => &["shaded_filter"],
    }
}

/// The function an application writes, for `mode`.
pub fn entry(mode: ShaderMode) -> &'static str {
    match mode {
        ShaderMode::Paint => "shade",
        ShaderMode::Coverage => "coverage",
        ShaderMode::Filter => "apply",
    }
}

/// The fragment stage a pipeline is built from, for `mode`.
pub fn fragment_entry(mode: ShaderMode) -> &'static str {
    match mode {
        ShaderMode::Paint => "fs_shaded_paint",
        ShaderMode::Coverage => "fs_shaded_coverage",
        ShaderMode::Filter => "fs_shaded_filter",
    }
}

/// The vertex stage a pipeline is built from.
///
/// A rectangle is four corners of its own box expanded in the vertex stage; a filter covers its
/// whole target and is cut to the region by the scissor, exactly as a blur is.
pub fn vertex_entry(mode: ShaderMode) -> &'static str {
    match mode {
        ShaderMode::Paint | ShaderMode::Coverage => "vs_shaded",
        ShaderMode::Filter => "vs_shaded_filter",
    }
}

#[cfg(test)]
mod tests {
    use super::{entry, epilogue, fragment_entry};
    use zgui_scene::ShaderMode;

    #[test]
    fn every_mode_has_its_own_function_and_its_own_fragment_stage() {
        let mut names: Vec<&str> = ShaderMode::ALL.iter().map(|mode| entry(*mode)).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), ShaderMode::ALL.len());

        let mut stages: Vec<&str> = ShaderMode::ALL
            .iter()
            .map(|mode| fragment_entry(*mode))
            .collect();
        stages.sort_unstable();
        stages.dedup();
        assert_eq!(stages.len(), ShaderMode::ALL.len());
    }

    #[test]
    fn every_rectangle_mode_is_built_from_the_shared_instance_and_one_stage_of_its_own() {
        for mode in ShaderMode::ALL
            .into_iter()
            .filter(|mode| mode.is_primitive())
        {
            let parts = epilogue(mode);
            assert_eq!(parts.first(), Some(&"shaded"), "{mode:?}");
            assert_eq!(parts.len(), 2, "{mode:?}");
        }
    }

    /// A filter is no rectangle, so it shares none of the instance the other two are built from.
    #[test]
    fn a_filter_is_built_from_its_own_stage_alone() {
        assert_eq!(epilogue(ShaderMode::Filter), &["shaded_filter"]);
    }
}
