#ifndef NSDK_CARAPACE_BRIDGE_H
#define NSDK_CARAPACE_BRIDGE_H

#include <stdbool.h>
#include <stdint.h>

/* Thin C bridge between the Native SDK host app (Zig) and carapace-ffi.
 *
 * Keeps the IOSurface pool, the carapace engine handle, and the render-thread
 * frame_ready handoff on the C side, where the IOSurface / CoreFoundation APIs
 * and the carapace C ABI are natural. The Zig app calls `cb_start` once and
 * `cb_latest_rgba` each gpu_surface frame.
 *
 * Threading: carapace fires `frame_ready` on ITS render thread (we only stash
 * an atomic index there). `cb_latest_rgba` runs on the Native SDK main thread
 * and is the only place that reads a surface and calls `carapace_release_surface`
 * — never from the render-thread callback (the ABI forbids reentrant carapace_*). */

/* Create the IOSurface pool (device pixels w*h, BGRA) and a carapace engine on
 * `skin_dir`. Returns 0 on success, or a negative code on failure:
 *   -1 already started, -2 IOSurface alloc failed, -3 carapace_create failed. */
int cb_start(const char *skin_dir, uint32_t w, uint32_t h);

/* If a carapace frame has landed, read the latest one into `out` as tight-packed
 * RGBA8 (device pixels w*h, top-left origin, opaque alpha) and return true.
 * Returns false if no frame is ready yet or `out_len` < w*h*4. */
bool cb_latest_rgba(uint8_t *out, uintptr_t out_len);

/* Device-pixel dimensions of the carapace surfaces (0 before cb_start). */
void cb_dims(uint32_t *w, uint32_t *h);

/* Debug: write the latest carapace frame to `path` as a binary PPM (P6, RGB).
 * Returns 0 on success, negative otherwise. Deterministic proof of the pixels
 * carapace produced, without screen-capturing anything. */
int cb_dump_ppm(const char *path);

/* Forward a pointer press at device-pixel (px,py) to carapace (maps to the skin
 * canvas internally). No-op before cb_start. Used by the input milestone. */
void cb_pointer(double px, double py);

/* Destroy the engine and release the pool. */
void cb_stop(void);

#endif /* NSDK_CARAPACE_BRIDGE_H */
