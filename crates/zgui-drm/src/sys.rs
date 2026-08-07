//! The generated interface to the kernel's DRM headers.
//!
//! DERIVED-FROM: the Linux kernel DRM uapi headers, MIT License
//!
//! `uapi/drm.h`, `uapi/drm_mode.h` and `uapi/drm_fourcc.h` are copied from the Linux kernel's
//! `include/uapi/drm/` and are covered by the MIT License. This module is the generated Rust form
//! of them, produced by `build.rs`, and it carries the attribution because neither a `.h` file nor
//! anything under `target/` is read by `cargo xtask ledger attribution`.
//!
//! Nothing here is documented or named the way the rest of this crate is. The module is private,
//! and the safe interface over it is hand-written.

// The module is private, so every `pub` bindgen writes is unreachable, and the headers declare
// enum typedefs this crate never names. Both are the generator's business.
#![allow(
    missing_docs,
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    dead_code,
    unreachable_pub,
    unused_imports
)]
// bindgen writes `unsafe { zeroed() }` for the `Default` of every struct that holds a union or an
// array. The rule that an unsafe block states what makes it sound applies to the blocks this
// crate writes. Sound here is bindgen's claim: it derives `Default` only where all-zero is a
// value of the type.
#![allow(clippy::undocumented_unsafe_blocks)]

include!(concat!(env!("OUT_DIR"), "/uapi.rs"));

#[cfg(test)]
mod tests {
    //! What the generated interface has to agree with the headers about.
    //!
    //! A request number is `_IOWR(type, nr, sizeof(struct))`. A struct that came out the wrong
    //! size therefore produces a *different request number*, which the kernel refuses with
    //! `EINVAL` and no further explanation. These are the sizes read out of the headers on a
    //! 64-bit build, and they are the same on 32-bit, because DRM declares its user pointers
    //! `__u64` rather than as pointers for exactly that reason.
    //!
    //! A constant that came out wrong fails more quietly. The call is accepted and the kernel
    //! does something else: a capability nobody asked for, a flag that means another flag. So the
    //! values are asserted here beside the sizes: the capabilities
    //! `DRM_CLIENT_CAP_UNIVERSAL_PLANES`, `DRM_CLIENT_CAP_ATOMIC`, `DRM_CAP_DUMB_BUFFER` and
    //! `DRM_CAP_ADDFB2_MODIFIERS`, the flags `DRM_MODE_ATOMIC_TEST_ONLY`,
    //! `DRM_MODE_ATOMIC_NONBLOCK`, `DRM_MODE_ATOMIC_ALLOW_MODESET`, `DRM_MODE_PAGE_FLIP_EVENT`
    //! and `DRM_MODE_FB_MODIFIERS`, and the event type `DRM_EVENT_FLIP_COMPLETE`.

    use super::*;

    #[test]
    fn the_generated_structs_are_the_size_the_headers_say() {
        assert_eq!(size_of::<drm_get_cap>(), 16);
        assert_eq!(size_of::<drm_set_client_cap>(), 16);
        assert_eq!(size_of::<drm_mode_card_res>(), 64);
        assert_eq!(size_of::<drm_mode_get_connector>(), 80);
        assert_eq!(size_of::<drm_mode_get_encoder>(), 20);
        assert_eq!(size_of::<drm_mode_crtc>(), 104);
        assert_eq!(size_of::<drm_mode_get_plane_res>(), 16);
        assert_eq!(size_of::<drm_mode_get_plane>(), 32);
        assert_eq!(size_of::<drm_mode_obj_get_properties>(), 32);
        assert_eq!(size_of::<drm_mode_get_property>(), 64);
        assert_eq!(size_of::<drm_mode_atomic>(), 56);
        assert_eq!(size_of::<drm_mode_fb_cmd2>(), 104);
        assert_eq!(size_of::<drm_mode_create_dumb>(), 32);
        assert_eq!(size_of::<drm_mode_map_dumb>(), 16);
        assert_eq!(size_of::<drm_mode_destroy_dumb>(), 4);
        assert_eq!(size_of::<drm_mode_crtc_page_flip>(), 24);
        assert_eq!(size_of::<drm_prime_handle>(), 12);
        assert_eq!(size_of::<drm_mode_create_blob>(), 16);
        assert_eq!(size_of::<drm_event>(), 8);
        assert_eq!(size_of::<drm_event_vblank>(), 32);
        assert_eq!(size_of::<drm_mode_modeinfo>(), 68);
    }

    #[test]
    fn the_constants_the_interface_turns_on_are_the_ones_the_headers_name() {
        // A constant that came out wrong fails worse than a size. The call is accepted and the
        // kernel does something else: a capability nobody asked for, a flag that means another
        // flag. Nothing reports it.
        assert_eq!(DRM_CLIENT_CAP_UNIVERSAL_PLANES, 2);
        assert_eq!(DRM_CLIENT_CAP_ATOMIC, 3);
        assert_eq!(DRM_CAP_DUMB_BUFFER, 1);
        assert_eq!(DRM_CAP_ADDFB2_MODIFIERS, 16);
        assert_eq!(DRM_MODE_ATOMIC_TEST_ONLY, 0x100);
        assert_eq!(DRM_MODE_ATOMIC_NONBLOCK, 0x200);
        assert_eq!(DRM_MODE_ATOMIC_ALLOW_MODESET, 0x400);
        assert_eq!(DRM_MODE_PAGE_FLIP_EVENT, 0x1);
        assert_eq!(DRM_MODE_FB_MODIFIERS, 0x2);
        assert_eq!(DRM_EVENT_FLIP_COMPLETE, 2);
    }
}
