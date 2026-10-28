//! What one machine's libgbm answers when it is asked for a scanout buffer, layout by layout.
//!
//! A diagnostic rather than a demonstration. The layout a driver hands back is invisible from
//! above — a buffer that is tiled and says so is indistinguishable, from Rust, from one that is
//! tiled and says nothing — and the difference decides whether a second card can draw into it. So
//! this asks each way in turn and prints what came back.
//!
//! ```text
//! cargo run -p zgui-gbm --example layouts -- /dev/dri/card1
//! ```
//!
//! Read the **stride** beside the layout code. A pitch wider than `width x 4` is a tiled buffer
//! whatever the code says: a row of 1280 pixels needs 5120 bytes, and gen3 Intel rounds a tiled
//! surface's pitch up to a power of two, which is 8192.

#[cfg(target_os = "linux")]
use std::os::fd::AsFd;

#[cfg(target_os = "linux")]
use zgui_gbm::{Device, Layout, Library};

/// `XR24`, the format a scanout buffer is allocated under.
#[cfg(target_os = "linux")]
const FOURCC: u32 = u32::from_le_bytes(*b"XR24");

/// Says so and leaves, on a host where the crate it would ask is empty.
#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("libgbm is a Linux interface, so there is nothing here to ask");
}

#[cfg(target_os = "linux")]
fn main() {
    let node = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/dev/dri/card0".to_owned());
    let (width, height) = (1280_u32, 1024_u32);

    let library = match Library::load() {
        Ok(library) => library,
        Err(reason) => {
            println!("libgbm did not load: {reason}");
            return;
        }
    };

    let card = match std::fs::File::options().read(true).write(true).open(&node) {
        Ok(card) => card,
        Err(error) => {
            println!("{node} did not open: {error}");
            return;
        }
    };
    let device = match Device::new(&library, card.as_fd()) {
        Ok(device) => device,
        Err(reason) => {
            println!("{node} is no allocator: {reason}");
            return;
        }
    };

    println!("{node}: gbm backend {}", device.backend());
    println!(
        "this libgbm {} be asked for a buffer by naming its layouts",
        if library.names_layouts() {
            "can"
        } else {
            "cannot"
        }
    );
    println!("a row of {width} pixels needs {} bytes", width * 4);
    for layout in [Layout::Driver, Layout::Linear] {
        match device.create(width, height, FOURCC, layout) {
            Ok(allocation) => println!(
                "{layout:?}: layout {:#x}, stride {}, offset {}",
                allocation.modifier(),
                allocation.stride(),
                allocation.offset()
            ),
            Err(reason) => println!("{layout:?}: refused, {reason}"),
        }
    }
}
