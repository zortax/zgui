/* The two headers bindgen is pointed at for the input interface. `input.h` pulls in
 * `input-event-codes.h` itself, and `uinput.h` pulls in `input.h`, so naming these two reaches
 * three of the five files beside this one.
 *
 * `time.h` is deliberately absent. It is read on its own, by a second pass in `build.rs`,
 * because it and the C library's `sys/time.h` — which `input.h` includes for `struct timeval` —
 * both define `timeval`, `itimerval` and `timezone`, and no translation unit can hold both.
 *
 * They are named through `linux/` because that is how `uinput.h` names `input.h`, and the three
 * headers beside this one are copied unchanged. `build.rs` stages them into that one directory
 * level so the copy resolves against itself rather than against whatever the host has installed.
 *
 * The five headers beside this one are copied unchanged from the Linux kernel's
 * `include/uapi/linux/`, at version 7.0. */
#include <linux/input.h>
#include <linux/uinput.h>
