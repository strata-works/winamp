//! Tier-1 proof without Swift: build an IOSurface in Rust, drive the C ABI, dump the surface.
//!
//! Creates a BGRA8 IOSurface, feeds it to `carapace_create`, ticks once, then reads the
//! surface memory back (un-swizzling BGRA→RGBA) and saves it as a PNG.
//! Reports the active tier (1 or 2) and asserts the green value bar is visible.

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("embed-spike examples are macOS-only");
}

#[cfg(target_os = "macos")]
fn main() {
    #![allow(deprecated)] // io-surface 0.16 intentionally used

    use std::ffi::{c_char, c_void, CStr, CString};

    use core_foundation::base::TCFType;
    use core_foundation::dictionary::CFDictionary;
    use core_foundation::number::CFNumber;
    use core_foundation::string::CFString;
    use io_surface::{
        IOSurfaceGetBaseAddress, IOSurfaceGetBytesPerRow, IOSurfaceLock, IOSurfaceUnlock,
        kIOSurfaceBytesPerElement, kIOSurfaceHeight, kIOSurfacePixelFormat, kIOSurfaceWidth,
    };

    use embed_spike::host::CarapaceHostVTable;

    extern "C" fn get_num(_ctx: *mut c_void, key: *const c_char, out: *mut f64) -> bool {
        if unsafe { CStr::from_ptr(key) }.to_str() == Ok("level") {
            unsafe { *out = 0.6 };
            true
        } else {
            false
        }
    }

    let (w, h) = (240u32, 80u32);

    // Create a BGRA8 IOSurface ('BGRA' = 0x42475241).
    // IOSurface properties dictionary uses CFString keys and CFType values.
    let props: CFDictionary<CFString, core_foundation::base::CFType> =
        CFDictionary::from_CFType_pairs(&[
            (
                unsafe { CFString::wrap_under_get_rule(kIOSurfaceWidth) },
                CFNumber::from(w as i64).as_CFType(),
            ),
            (
                unsafe { CFString::wrap_under_get_rule(kIOSurfaceHeight) },
                CFNumber::from(h as i64).as_CFType(),
            ),
            (
                unsafe { CFString::wrap_under_get_rule(kIOSurfaceBytesPerElement) },
                CFNumber::from(4i64).as_CFType(),
            ),
            (
                // 'BGRA' pixel format
                unsafe { CFString::wrap_under_get_rule(kIOSurfacePixelFormat) },
                CFNumber::from(0x4247_5241i64).as_CFType(),
            ),
        ]);
    let surface = io_surface::new(&props);
    let surface_ref = surface.obj; // *const __IOSurface == IOSurfaceRef

    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("skin");
    let cdir = CString::new(dir.to_str().unwrap()).unwrap();
    let vtable = CarapaceHostVTable {
        ctx: std::ptr::null_mut(),
        get_num: Some(get_num),
        get_str: None,
        invoke: None,
    };

    unsafe {
        // No content surface for the headless harness: pass a null IOSurfaceRef so the engine
        // supplies no host content (this skin has no view{} anyway).
        let null_content = std::ptr::null() as io_surface::IOSurfaceRef;
        let e =
            embed_spike::carapace_create(cdir.as_ptr(), vtable, surface_ref, null_content, w, h);
        assert!(!e.is_null(), "carapace_create returned null");

        embed_spike::carapace_tick(e, 0.016);

        // This headless harness creates its own BGRA IOSurface, so with Task 6 it may now
        // reach Tier 2 (zero-copy import). Accept whichever tier is reached and just report it;
        // the real proof below is that the green bar lands in the surface with correct colors.
        let tier = embed_spike::carapace_active_tier(e);
        assert!(tier == 1 || tier == 2, "unexpected tier {tier}");
        println!("active tier: {tier}");

        // Read the surface back and un-swizzle BGRA → RGBA for the PNG.
        let mut seed: u32 = 0;
        IOSurfaceLock(
            surface_ref,
            io_surface::IOSurfaceLockOptions::kIOSurfaceLockReadOnly,
            &mut seed,
        );
        let base = IOSurfaceGetBaseAddress(surface_ref) as *const u8;
        let stride = IOSurfaceGetBytesPerRow(surface_ref);
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        for y in 0..h as usize {
            for x in 0..w as usize {
                let p = base.add(y * stride + x * 4);
                let out = &mut rgba[(y * w as usize + x) * 4..];
                // Surface is BGRA: p[0]=B, p[1]=G, p[2]=R, p[3]=A
                // PNG wants RGBA: out[0]=R, out[1]=G, out[2]=B, out[3]=A
                out[0] = *p.add(2); // R
                out[1] = *p.add(1); // G
                out[2] = *p;        // B
                out[3] = *p.add(3); // A
            }
        }
        IOSurfaceUnlock(
            surface_ref,
            io_surface::IOSurfaceLockOptions::kIOSurfaceLockReadOnly,
            &mut seed,
        );

        embed_spike::carapace_destroy(e);

        // The green value bar (~120,230,80) must be present.
        let has_green = rgba
            .chunks_exact(4)
            .any(|p| p[1] > 180 && p[0] < 180 && p[2] < 160 && p[3] > 0);
        assert!(has_green, "value bar not visible in the IOSurface — check swizzle");

        std::fs::create_dir_all("target").unwrap();
        image::save_buffer("target/iosurface_png.png", &rgba, w, h, image::ColorType::Rgba8)
            .unwrap();
        println!("wrote target/iosurface_png.png");
    }
}
