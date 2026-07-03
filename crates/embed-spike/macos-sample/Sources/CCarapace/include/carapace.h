#ifndef CARAPACE_H
#define CARAPACE_H
#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>
#include <IOSurface/IOSurfaceRef.h>

typedef struct CarapaceEngine CarapaceEngine;

/* Byte-identical to the Rust #[repr(C)] CarapaceHostVTable:
 *   ctx:     *mut c_void          — 8 bytes
 *   get_num: Option<fn(...)>      — 8 bytes (function pointer)
 *   get_str: Option<fn(...)>      — 8 bytes
 *   invoke:  Option<fn(...)>      — 8 bytes
 */
typedef struct {
  void* ctx;
  bool (*get_num)(void* ctx, const char* key, double* out);
  bool (*get_str)(void* ctx, const char* key, char* buf, size_t cap);
  void (*invoke)(void* ctx, const char* action);
} CarapaceHostVTable;

/* Create the engine. Returns NULL on failure.
 * surface must be a valid IOSurface (BGRA8, w×h pixels) that outlives the engine.
 * content_surface, if non-NULL, is a BGRA8 IOSurface holding the host's own live content;
 * the engine composites it into the skin's view{ id = "host" } cutout. Pass NULL for none. */
CarapaceEngine* carapace_create(const char* skin_dir, CarapaceHostVTable host,
                                IOSurfaceRef surface, IOSurfaceRef content_surface,
                                uint32_t w, uint32_t h);

/* Tick the engine by dt_seconds and composite one frame into the IOSurface. */
void carapace_tick(CarapaceEngine* e, double dt_seconds);

/* Forward a pointer event (canvas coords). kind: 0 = press. */
void carapace_pointer(CarapaceEngine* e, double x, double y, int32_t kind);

/* Returns 1 = Readback (CPU copy into IOSurface), 2 = Shared (zero-copy Metal texture). */
int32_t carapace_active_tier(CarapaceEngine* e);

// Switch the paper-shader surround to the next vendored shader.
void carapace_cycle_shader(CarapaceEngine* e);

/* Destroy the engine. Do not use e after this call. */
void carapace_destroy(CarapaceEngine* e);

#endif /* CARAPACE_H */
