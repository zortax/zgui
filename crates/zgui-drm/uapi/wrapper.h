/* The two headers bindgen is pointed at. `drm.h` pulls in `drm_mode.h` itself, and
 * `drm_fourcc.h` pulls in `drm.h`, so naming these two reaches all three.
 *
 * The three headers beside this one are copied unchanged from the Linux kernel's
 * `include/uapi/drm/`, at version 7.0. */
#include "drm.h"
#include "drm_fourcc.h"
