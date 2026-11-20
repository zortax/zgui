//! zgui's renderer vocabulary and the compositor's, side by side.

use compositor_types as to;
use zgui_render as from;

/// Returns the compositor's outcome in zgui's vocabulary.
pub(crate) fn outcome(outcome: to::FrameOutcome) -> from::FrameOutcome {
    match outcome {
        to::FrameOutcome::Presented(stats) => from::FrameOutcome::Presented(from::FrameStats {
            draw_calls: stats.draw_calls,
            vector_passes: stats.vector_passes,
            damage_px: stats.damage_px,
            bytes_uploaded: stats.bytes_uploaded,
            memory: from::MemoryReport::default(),
        }),
        to::FrameOutcome::Skipped(reason) => from::FrameOutcome::Skipped(skip(reason)),
        to::FrameOutcome::Recovered => from::FrameOutcome::Recovered,
        // A variant added after this was written keeps the damage and asks for another frame.
        _ => from::FrameOutcome::Skipped(from::SkipReason::Unacquired),
    }
}

/// Returns the skip reason of the same name.
fn skip(reason: to::SkipReason) -> from::SkipReason {
    use from::SkipReason as Z;
    use to::SkipReason as C;
    match reason {
        C::Unconfigured => Z::Unconfigured,
        C::Timeout => Z::Timeout,
        C::Declined => Z::Declined,
        C::Unacquired => Z::Unacquired,
        C::Occluded => Z::Occluded,
        C::Outdated => Z::Outdated,
        C::Validation => Z::Validation,
        C::DeviceUnavailable => Z::DeviceUnavailable,
        C::Undamaged => Z::Undamaged,
        C::Held => Z::Held,
        _ => Z::Unacquired,
    }
}

/// Returns the device's capabilities in zgui's vocabulary.
pub(crate) fn capabilities(caps: to::RenderCapabilities) -> from::RenderCapabilities {
    let mut out = from::RenderCapabilities::MINIMAL;
    out.subpixel_text = caps.subpixel_text;
    out.vector_compute = caps.vector_compute;
    out.custom_shaders = caps.custom_shaders;
    out.max_texture_size = i32::try_from(caps.max_texture_size).unwrap_or(i32::MAX);
    out
}

/// Returns a render target in the compositor's vocabulary.
pub(crate) fn target(target: from::RenderTarget) -> to::RenderTarget {
    to::RenderTarget {
        size: [
            u32::try_from(target.size.width).unwrap_or(0),
            u32::try_from(target.size.height).unwrap_or(0),
        ],
        scale: target.scale.get(),
        opaque: target.opaque,
    }
}

/// Returns the device memory in zgui's vocabulary.
pub(crate) fn memory(memory: to::MemoryReport) -> from::MemoryReport {
    from::MemoryReport {
        fixed: memory.fixed,
        targets: memory.targets,
        scratch: memory.vector,
        atlases: memory.atlas,
        buffers: memory.chunks,
    }
}

/// Returns the stripe order of the same name.
pub(crate) fn subpixel(order: from::SubpixelOrder) -> to::SubpixelOrder {
    match order {
        from::SubpixelOrder::RedToBlue => to::SubpixelOrder::RedToBlue,
        from::SubpixelOrder::BlueToRed => to::SubpixelOrder::BlueToRed,
    }
}

/// Returns the path rasteriser's state in zgui's vocabulary.
pub(crate) fn vector_status(status: to::VectorStatus) -> from::VectorStatus {
    from::VectorStatus {
        backend: status.backend.map(|backend| match backend {
            to::VectorBackend::Compute => from::VectorBackend::Vello,
            to::VectorBackend::Raster => from::VectorBackend::Coverage,
            _ => from::VectorBackend::Other,
        }),
        initialized: status.initialized,
    }
}
