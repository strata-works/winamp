//! One-off asset tool: alpha-cut the Headspace faceplate's baked background so the skin floats
//! in the borderless window. Flood-fills from the four corners, joining a pixel to the
//! background when it is within `TOL` (per channel) of an already-background neighbor — so it
//! follows the smooth sky gradient but stops at the hard green head/speaker edges.
//!
//! Run: `cargo run -p carapace-demo --example cut_headspace [TOL]`  (default TOL=24)
//! Overwrites `skins/reference/assets/headspace.png`. The original is recoverable via git.

use image::{Rgba, RgbaImage};

fn main() {
    let tol: i32 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(24);

    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/skins/reference/assets/headspace.png"
    );
    let img: RgbaImage = image::open(path).expect("open headspace.png").to_rgba8();
    let (w, h) = img.dimensions();
    let idx = |x: u32, y: u32| (y * w + x) as usize;
    let near = |a: &Rgba<u8>, b: &Rgba<u8>| {
        (a[0] as i32 - b[0] as i32).abs() <= tol
            && (a[1] as i32 - b[1] as i32).abs() <= tol
            && (a[2] as i32 - b[2] as i32).abs() <= tol
    };

    let mut bg = vec![false; (w * h) as usize];
    let mut stack: Vec<(u32, u32)> = Vec::new();
    for (x, y) in [(0, 0), (w - 1, 0), (0, h - 1), (w - 1, h - 1)] {
        bg[idx(x, y)] = true;
        stack.push((x, y));
    }
    while let Some((x, y)) = stack.pop() {
        let cur = *img.get_pixel(x, y);
        let mut neighbors = Vec::with_capacity(4);
        if x > 0 {
            neighbors.push((x - 1, y));
        }
        if x + 1 < w {
            neighbors.push((x + 1, y));
        }
        if y > 0 {
            neighbors.push((x, y - 1));
        }
        if y + 1 < h {
            neighbors.push((x, y + 1));
        }
        for (nx, ny) in neighbors {
            let i = idx(nx, ny);
            if !bg[i] && near(&cur, img.get_pixel(nx, ny)) {
                bg[i] = true;
                stack.push((nx, ny));
            }
        }
    }

    let mut out = img.clone();
    let mut cut = 0usize;
    for y in 0..h {
        for x in 0..w {
            if bg[idx(x, y)] {
                out.put_pixel(x, y, Rgba([0, 0, 0, 0]));
                cut += 1;
            }
        }
    }
    out.save(path).expect("save headspace.png");
    println!(
        "TOL={tol}: cut {cut} of {} px ({:.1}%) to transparent",
        w * h,
        100.0 * cut as f32 / (w * h) as f32
    );
}
