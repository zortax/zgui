/* The two headers bindgen is pointed at. `input.h` pulls in `input-event-codes.h` itself, and
 * `uinput.h` pulls in `input.h`, so naming these two reaches all three.
 *
 * They are named through `linux/` because that is how `uinput.h` names `input.h`, and the three
 * headers beside this one are copied unchanged. `build.rs` stages them into that one directory
 * level so the copy resolves against itself rather than against whatever the host has installed.
 *
 * The three headers beside this one are copied unchanged from the Linux kernel's
 * `include/uapi/linux/`, at version 7.0. */
#include <linux/input.h>
#include <linux/uinput.h>
