#include "carapace_bridge.h"

#include <stdatomic.h>
#include <stdio.h>
#include <string.h>

#include <CoreFoundation/CoreFoundation.h>
#include <IOSurface/IOSurface.h>

/* carapace.h gates its Apple-only create desc + exports behind CARAPACE_APPLE
 * (see cbindgen.toml); build.zig passes -DCARAPACE_APPLE for this TU. */
#include "carapace.h"

#define POOL_COUNT 3

static struct {
    int started;
    CarapaceEngine *engine;
    IOSurfaceRef surfaces[POOL_COUNT];
    uint32_t w, h;
    _Atomic uint32_t latest; /* index of the newest frame_ready surface */
    _Atomic int have;        /* 0 until the first frame_ready */
    int32_t held;            /* surface currently pinned for reading (-1 = none) */
} g = {.held = -1};

/* Runs on carapace's render thread. MUST be non-blocking and MUST NOT call any
 * carapace_* (would reenter the queue). Just stash the index. */
static void on_frame_ready(void *ctx, uint32_t index, uint64_t frame_id) {
    (void)ctx;
    (void)frame_id;
    atomic_store(&g.latest, index);
    atomic_store(&g.have, 1);
}

static IOSurfaceRef make_bgra_surface(uint32_t w, uint32_t h) {
    CFMutableDictionaryRef props = CFDictionaryCreateMutable(
        kCFAllocatorDefault, 0, &kCFTypeDictionaryKeyCallBacks, &kCFTypeDictionaryValueCallBacks);
    if (!props) return NULL;

    int32_t ww = (int32_t)w, hh = (int32_t)h, bpe = 4, bpp = 4;
    int32_t stride = (int32_t)(w * 4);
    int32_t pf = 0x42475241; /* 'BGRA' */

#define SET_I32(key, valp)                                                          \
    do {                                                                            \
        CFNumberRef n = CFNumberCreate(kCFAllocatorDefault, kCFNumberSInt32Type, valp); \
        CFDictionarySetValue(props, key, n);                                        \
        CFRelease(n);                                                               \
    } while (0)

    SET_I32(kIOSurfaceWidth, &ww);
    SET_I32(kIOSurfaceHeight, &hh);
    SET_I32(kIOSurfaceBytesPerElement, &bpe);
    SET_I32(kIOSurfaceBytesPerRow, &stride);
    SET_I32(kIOSurfacePixelFormat, &pf);
    (void)bpp;
#undef SET_I32

    IOSurfaceRef s = IOSurfaceCreate(props);
    CFRelease(props);
    return s;
}

int cb_start(const char *skin_dir, uint32_t w, uint32_t h) {
    if (g.started) return -1;
    if (w == 0 || h == 0) return -2;
    g.w = w;
    g.h = h;

    for (int i = 0; i < POOL_COUNT; i++) {
        g.surfaces[i] = make_bgra_surface(w, h);
        if (!g.surfaces[i]) return -2;
    }

    CarapaceHostVTable vt;
    memset(&vt, 0, sizeof(vt)); /* all host-data callbacks null -> engine uses defaults */
    vt.frame_ready = on_frame_ready;

    CarapaceCreateDesc desc;
    memset(&desc, 0, sizeof(desc));
    desc.skin_dir = skin_dir;
    desc.vtable = vt;
    desc.surfaces = g.surfaces;
    desc.surface_count = POOL_COUNT;
    desc.content_surface = NULL;
    desc.w = w;
    desc.h = h;

    CarapaceEngine *engine = NULL;
    if (carapace_create(&desc, &engine) != Ok || engine == NULL) {
        return -3;
    }
    g.engine = engine;
    g.started = 1;
    return 0;
}

void cb_dims(uint32_t *w, uint32_t *h) {
    if (w) *w = g.w;
    if (h) *h = g.h;
}

bool cb_latest_rgba(uint8_t *out, uintptr_t out_len) {
    if (!g.started || !atomic_load(&g.have)) return false;
    const uint32_t w = g.w, h = g.h;
    const uintptr_t need = (uintptr_t)w * h * 4;
    if (out_len < need) return false;

    uint32_t idx = atomic_load(&g.latest);
    if (idx >= POOL_COUNT) return false;
    IOSurfaceRef s = g.surfaces[idx];

    IOSurfaceLock(s, kIOSurfaceLockReadOnly, NULL);
    const uint8_t *base = (const uint8_t *)IOSurfaceGetBaseAddress(s);
    const size_t stride = IOSurfaceGetBytesPerRow(s);
    /* BGRA (bytes B,G,R,A) -> RGBA (bytes R,G,B,A); force opaque A (the pane is
     * opaque and vello output is premultiplied, so straight-alpha edges would
     * otherwise darken). Handles a padded IOSurface stride row by row. */
    for (uint32_t y = 0; y < h; y++) {
        const uint8_t *src = base + (size_t)y * stride;
        uint8_t *dst = out + (size_t)y * w * 4;
        for (uint32_t x = 0; x < w; x++) {
            dst[0] = src[2];
            dst[1] = src[1];
            dst[2] = src[0];
            dst[3] = 255;
            src += 4;
            dst += 4;
        }
    }
    IOSurfaceUnlock(s, kIOSurfaceLockReadOnly, NULL);

    /* Release the previously-pinned surface now that we've moved on to a newer
     * one, so carapace can reuse it. Keep the current one pinned until next time. */
    if (g.held >= 0 && (uint32_t)g.held != idx) {
        carapace_release_surface(g.engine, (uint32_t)g.held);
    }
    g.held = (int32_t)idx;
    return true;
}

int cb_dump_ppm(const char *path) {
    if (!g.started || !atomic_load(&g.have)) return -1;
    uint32_t idx = atomic_load(&g.latest);
    if (idx >= POOL_COUNT) return -1;
    IOSurfaceRef s = g.surfaces[idx];
    FILE *f = fopen(path, "wb");
    if (!f) return -2;
    fprintf(f, "P6\n%u %u\n255\n", g.w, g.h);
    IOSurfaceLock(s, kIOSurfaceLockReadOnly, NULL);
    const uint8_t *base = (const uint8_t *)IOSurfaceGetBaseAddress(s);
    const size_t stride = IOSurfaceGetBytesPerRow(s);
    for (uint32_t y = 0; y < g.h; y++) {
        const uint8_t *src = base + (size_t)y * stride;
        for (uint32_t x = 0; x < g.w; x++) {
            uint8_t rgb[3] = {src[2], src[1], src[0]}; /* BGRA -> RGB */
            fwrite(rgb, 1, 3, f);
            src += 4;
        }
    }
    IOSurfaceUnlock(s, kIOSurfaceLockReadOnly, NULL);
    fclose(f);
    return 0;
}

void cb_pointer(double px, double py) {
    if (!g.started) return;
    /* carapace_pointer takes canvas-space coords; the engine's snapshot maps
     * device px internally for hit-testing, but pointer() expects canvas units.
     * The surface was created at the skin's native canvas*scale, and for the
     * spike the pane matches the surface 1:1, so px/py in device pixels map to
     * canvas via the engine's own scale. Pass device px through; carapace scales. */
    (void)carapace_pointer(g.engine, px, py, Press);
}

void cb_stop(void) {
    if (!g.started) return;
    carapace_destroy(g.engine);
    g.engine = NULL;
    for (int i = 0; i < POOL_COUNT; i++) {
        if (g.surfaces[i]) {
            CFRelease(g.surfaces[i]);
            g.surfaces[i] = NULL;
        }
    }
    g.started = 0;
    g.held = -1;
    atomic_store(&g.have, 0);
}
