#ifndef NSDK_CLINK_SHIM_H
#define NSDK_CLINK_SHIM_H

#include <stdint.h>

/* Minimal external-C-library surface for the Native SDK build-path proof.
 * This is where IOSurface / Metal zero-copy helpers will live once the
 * carapace-ffi integration lands. For now it just proves that an app-owned
 * C translation unit compiles, links, and is callable from Zig. */
uint32_t shim_answer(void);

#endif /* NSDK_CLINK_SHIM_H */
