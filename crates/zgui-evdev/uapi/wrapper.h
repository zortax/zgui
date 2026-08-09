/* The four headers bindgen is pointed at. `input.h` pulls in `input-event-codes.h` itself, and
 * `uinput.h` pulls in `input.h`, so the first two names reach three files. `keyboard.h` pulls in
 * `wait.h`, so the last two reach three more.
 *
 * `kd.h` and `keyboard.h` are the console's keymap interface. `kd.h` holds `struct kbentry`, the
 * request numbers and the keyboard modes; `keyboard.h` holds the entry types, the modifier bits
 * and the number of keys a console keymap has. They share this translation unit with the input
 * headers because the four define no name twice.
 *
 * `time.h` is deliberately absent. It is read on its own, by a second pass in `build.rs`,
 * because it and the C library's `sys/time.h` — which `input.h` includes for `struct timeval` —
 * both define `timeval`, `itimerval` and `timezone`, and no translation unit can hold both.
 *
 * They are named through `linux/` because that is how `uinput.h` names `input.h`, and the
 * headers beside this one are copied unchanged. `build.rs` stages them into that one directory
 * level so the copy resolves against itself rather than against whatever the host has installed.
 *
 * The eight headers beside this one are copied unchanged from the Linux kernel's
 * `include/uapi/linux/`, at version 7.0. */
#include <linux/input.h>
#include <linux/uinput.h>
#include <linux/kd.h>
#include <linux/keyboard.h>

/* `keyboard.h` builds every named keymap entry from `K(t,v)`, and bindgen evaluates no constant
 * built from a function-like macro, so none of them reaches Rust. The two this crate names are
 * restated here as an enumeration, which the compiler folds. So the values still come from the
 * header, and `pack` in `console.rs` is checked against the header's own `K(t,v)` rather than
 * against a number copied beside it. */
enum zgui_console_entry {
	ZGUI_K_HOLE = K_HOLE,
	ZGUI_K_NOSUCHMAP = K_NOSUCHMAP,
};
