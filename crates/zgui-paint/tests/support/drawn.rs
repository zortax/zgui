//! Reading the fields of instances a compositor frame draws.

use zgui_testkit_compositor::Instance;

/// Returns the field `name` of `instance` as the numbers of the list `[a, b, ...]` it holds.
pub(crate) fn numbers(instance: &Instance, name: &str) -> Option<Vec<f32>> {
    let inner = instance.field(name)?.strip_prefix('[')?.strip_suffix(']')?;
    inner
        .split(',')
        .map(|number| number.trim().parse::<f32>().ok())
        .collect()
}

/// Returns the field `name` of `instance` as the one number it holds.
pub(crate) fn number(instance: &Instance, name: &str) -> Option<f32> {
    instance.field(name)?.parse().ok()
}
