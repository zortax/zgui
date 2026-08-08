//! Choosing an adapter, and refusing to guess when none of them works.

use std::sync::atomic::{AtomicU64, Ordering};

use zgui_render::GpuUnavailable;

/// The environment variable naming the PCI device id to prefer.
///
/// A machine with two adapters renders on whichever one the sort below prefers, and on a hybrid
/// laptop that is not always the one the compositor is on. This is the escape hatch, and it is a
/// preference rather than a filter: an id that matches nothing still selects normally, because a
/// stale value in a shell profile must not turn into "this program has no graphics device".
pub const DEVICE_ID_VARIABLE: &str = "ZGUI_DEVICE_ID";

/// The environment variable restricting which backends are enumerated at all.
///
/// Accepts `vulkan`, `metal`, `dx12`, `gl`, a comma-separated list of them, or `none`. `none` is
/// what makes the no-device path reachable on a machine that has one: without it, the only way to
/// see what a user with no driver sees is to uninstall the driver.
pub const BACKENDS_VARIABLE: &str = "ZGUI_BACKENDS";

/// The backends enumerated when nothing restricts them.
///
/// Each platform has one primary — Metal on Apple, DX12 on Windows, Vulkan elsewhere — and GL
/// rides along as the fallback for a machine whose primary has no working driver. wgpu reaches
/// GL through EGL, which Apple platforms do not ship, so the Apple set is Metal alone. GL is
/// enumerated only after the primary has failed — see [`tiers`].
pub fn default_backends() -> wgpu::Backends {
    #[cfg(target_vendor = "apple")]
    {
        wgpu::Backends::METAL
    }
    #[cfg(target_os = "windows")]
    {
        wgpu::Backends::DX12 | wgpu::Backends::GL
    }
    #[cfg(not(any(target_vendor = "apple", target_os = "windows")))]
    {
        wgpu::Backends::VULKAN | wgpu::Backends::GL
    }
}

/// How many times a backend set containing GL has been enumerated in this process.
static GL_ENUMERATIONS: AtomicU64 = AtomicU64::new(0);

/// How many times this process has asked a driver for its GL adapters.
///
/// Enumerating GL loads, initialises and interrogates a vendor's EGL and GL cores, which on a
/// discrete card costs more than everything else about choosing an adapter put together. It is a
/// fallback, so on a machine where the primary works this number is zero, and that is a thing a
/// test can assert rather than a thing a profile has to be read for.
pub fn gl_enumerations() -> u64 {
    GL_ENUMERATIONS.load(Ordering::Relaxed)
}

/// The backend sets to enumerate, in the order they are to be tried.
///
/// A fallback constructed before the primary has been tried is not a fallback, it is a tax: the
/// adapters of every requested backend used to be enumerated up front, so a machine that opens a
/// native device every time still paid for its GL cores to be brought up and then dropped unused.
/// So the request is cut into tiers — the native backends, then GL — and a tier is enumerated
/// only once every tier before it has failed to produce a working device.
///
/// A request naming one backend yields one tier, which is what makes [`BACKENDS_VARIABLE`] mean
/// exactly what it says. The empty request yields no tiers at all.
pub fn tiers(backends: wgpu::Backends) -> Vec<wgpu::Backends> {
    let primary = backends - wgpu::Backends::GL;
    let fallback = backends & wgpu::Backends::GL;
    [primary, fallback]
        .into_iter()
        .filter(|tier| !tier.is_empty())
        .collect()
}

/// The backends to enumerate, after the environment has had its say.
pub fn requested_backends() -> wgpu::Backends {
    match std::env::var(BACKENDS_VARIABLE) {
        Err(_) => default_backends(),
        Ok(value) => parse_backends(&value),
    }
}

/// Parses the backend list an environment variable carries.
///
/// An unrecognised word contributes nothing, so `ZGUI_BACKENDS=nonsense` is the same request as
/// `none`: a typo that quietly fell back to every backend would make this variable useless for
/// reproducing a machine with no usable device.
pub fn parse_backends(value: &str) -> wgpu::Backends {
    let mut backends = wgpu::Backends::empty();
    for word in value.split(',') {
        match word.trim().to_ascii_lowercase().as_str() {
            "vulkan" => backends |= wgpu::Backends::VULKAN,
            "metal" => backends |= wgpu::Backends::METAL,
            "dx12" | "d3d12" => backends |= wgpu::Backends::DX12,
            "gl" | "gles" | "opengl" => backends |= wgpu::Backends::GL,
            "all" => backends |= default_backends(),
            _ => {}
        }
    }
    backends
}

/// The preferred device id, if one was named.
pub fn preferred_device_id() -> Option<u32> {
    let value = std::env::var(DEVICE_ID_VARIABLE).ok()?;
    let value = value.trim();
    let parsed = match value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        Some(hex) => u32::from_str_radix(hex, 16),
        None => value.parse(),
    };
    parsed.ok()
}

/// How good a candidate an adapter looks, before anything has been created from it.
///
/// Lower sorts first. The order is: the device id the environment asked for, then discrete before
/// integrated before virtual before software, then a native backend before GL. It is only a
/// *sort*: every candidate is still tried in turn, because the capabilities an adapter reports are
/// not always the capabilities a device created from it has.
pub fn sort_key(info: &wgpu::AdapterInfo, preferred: Option<u32>) -> (u8, u8, u8) {
    let named = u8::from(Some(info.device) != preferred);
    let device_type = match info.device_type {
        wgpu::DeviceType::DiscreteGpu => 0,
        wgpu::DeviceType::IntegratedGpu => 1,
        wgpu::DeviceType::VirtualGpu => 2,
        wgpu::DeviceType::Cpu => 3,
        wgpu::DeviceType::Other => 4,
    };
    let backend = match info.backend {
        wgpu::Backend::Vulkan | wgpu::Backend::Metal | wgpu::Backend::Dx12 => 0,
        wgpu::Backend::Gl => 1,
        _ => 2,
    };
    (named, device_type, backend)
}

/// A one-line description of an adapter, for a rejection list or a startup line.
pub fn describe(info: &wgpu::AdapterInfo) -> String {
    format!(
        "{} ({:?}, {:?}, device 0x{:04x}, driver {} {})",
        info.name, info.device_type, info.backend, info.device, info.driver, info.driver_info
    )
}

/// Every adapter of `backends`, best-looking first.
///
/// This is one tier's worth of enumeration, not the whole request: see [`tiers`].
pub fn candidates(instance: &wgpu::Instance, backends: wgpu::Backends) -> Vec<wgpu::Adapter> {
    if backends.intersects(wgpu::Backends::GL) {
        GL_ENUMERATIONS.fetch_add(1, Ordering::Relaxed);
    }
    let preferred = preferred_device_id();
    let mut adapters = futures::executor::block_on(instance.enumerate_adapters(backends));
    adapters.sort_by_key(|adapter| sort_key(&adapter.get_info(), preferred));
    adapters
}

/// The failure to report when the candidate loop rejected everything.
///
/// It carries every adapter that was considered and why, because "no usable graphics device" with
/// no list is a bug report nobody can act on — and because the alternative, quietly rendering
/// offscreen, produces a window that appears and never paints.
pub fn unavailable(rejections: Vec<(String, String)>) -> GpuUnavailable {
    rejections
        .into_iter()
        .fold(GpuUnavailable::new(), |failure, (name, reason)| {
            failure.rejected(name, reason)
        })
}

#[cfg(test)]
mod tests {
    use super::{describe, parse_backends, sort_key, tiers, unavailable};

    #[test]
    fn gl_is_a_tier_of_its_own_behind_the_native_backends() {
        assert_eq!(
            tiers(wgpu::Backends::VULKAN | wgpu::Backends::GL),
            vec![wgpu::Backends::VULKAN, wgpu::Backends::GL],
            "the fallback is a second tier, not part of the first"
        );
        assert_eq!(
            tiers(wgpu::Backends::METAL | wgpu::Backends::GL),
            vec![wgpu::Backends::METAL, wgpu::Backends::GL],
            "every native backend outranks the fallback, whatever the platform"
        );
    }

    #[test]
    fn a_request_for_one_backend_is_one_tier() {
        assert_eq!(tiers(wgpu::Backends::GL), vec![wgpu::Backends::GL]);
        assert_eq!(tiers(wgpu::Backends::VULKAN), vec![wgpu::Backends::VULKAN]);
        assert!(tiers(wgpu::Backends::empty()).is_empty());
    }

    /// An adapter description with the fields the sort reads.
    fn info(
        device: u32,
        device_type: wgpu::DeviceType,
        backend: wgpu::Backend,
    ) -> wgpu::AdapterInfo {
        wgpu::AdapterInfo {
            name: "test".to_owned(),
            vendor: 0,
            device,
            device_type,
            driver: String::new(),
            driver_info: String::new(),
            backend,
            device_pci_bus_id: String::new(),
            subgroup_min_size: 32,
            subgroup_max_size: 32,
            transient_saves_memory: false,
        }
    }

    #[test]
    fn a_named_device_outranks_a_better_one() {
        let discrete = info(1, wgpu::DeviceType::DiscreteGpu, wgpu::Backend::Vulkan);
        let integrated = info(2, wgpu::DeviceType::IntegratedGpu, wgpu::Backend::Vulkan);
        assert!(sort_key(&discrete, None) < sort_key(&integrated, None));
        assert!(sort_key(&integrated, Some(2)) < sort_key(&discrete, Some(2)));
    }

    #[test]
    fn vulkan_outranks_gl_at_the_same_device_type() {
        let vulkan = info(1, wgpu::DeviceType::DiscreteGpu, wgpu::Backend::Vulkan);
        let gl = info(1, wgpu::DeviceType::DiscreteGpu, wgpu::Backend::Gl);
        assert!(sort_key(&vulkan, None) < sort_key(&gl, None));
    }

    #[test]
    fn software_sorts_last_but_is_still_a_candidate() {
        let cpu = info(1, wgpu::DeviceType::Cpu, wgpu::Backend::Vulkan);
        let virtualised = info(1, wgpu::DeviceType::VirtualGpu, wgpu::Backend::Vulkan);
        assert!(sort_key(&virtualised, None) < sort_key(&cpu, None));
    }

    #[test]
    fn an_unrecognised_backend_list_asks_for_nothing() {
        assert_eq!(parse_backends("vulkan"), wgpu::Backends::VULKAN);
        assert_eq!(parse_backends("metal"), wgpu::Backends::METAL);
        assert_eq!(parse_backends("dx12"), wgpu::Backends::DX12);
        assert_eq!(
            parse_backends("gl, vulkan"),
            wgpu::Backends::VULKAN | wgpu::Backends::GL
        );
        assert!(parse_backends("none").is_empty());
        assert!(parse_backends("nonsense").is_empty());
    }

    #[test]
    fn the_failure_lists_every_rejection_in_order() {
        let failure = unavailable(vec![
            ("first".to_owned(), "no device".to_owned()),
            ("second".to_owned(), "surface refused".to_owned()),
        ]);
        assert_eq!(failure.candidates.len(), 2);
        assert_eq!(failure.candidates[0].name, "first");
        assert_eq!(failure.candidates[1].reason, "surface refused");
    }

    #[test]
    fn a_description_names_the_backend_and_the_device() {
        let text = describe(&info(
            0x2216,
            wgpu::DeviceType::DiscreteGpu,
            wgpu::Backend::Gl,
        ));
        assert!(text.contains("Gl"), "{text}");
        assert!(text.contains("0x2216"), "{text}");
    }
}
