use hittest::{Point, Region};

pub mod vello_backend;

#[derive(Clone, Debug)]
pub struct Pixmap {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

pub trait Renderer {
    fn name(&self) -> &'static str;
    fn render(&mut self, region: &Region, size: (u32, u32), fill: [u8; 4], bg: [u8; 4]) -> Pixmap;
}

#[derive(Debug)]
pub struct ParityReport {
    pub checked: usize,
    pub mismatches: Vec<(u32, u32)>,
}

pub fn parity_check(region: &Region, pm: &Pixmap, fill: [u8; 4], bg: [u8; 4]) -> ParityReport {
    let mut checked = 0usize;
    let mut mismatches = Vec::new();
    for y in 0..pm.height {
        for x in 0..pm.width {
            let i = ((y * pm.width + x) * 4) as usize;
            let px = [pm.data[i], pm.data[i + 1], pm.data[i + 2], pm.data[i + 3]];
            let is_fill = px == fill;
            let is_bg = px == bg;
            if !is_fill && !is_bg {
                // Antialiased / blended edge pixel — ambiguous, skip it.
                continue;
            }
            checked += 1;
            let inside = region.contains(Point {
                x: x as f32 + 0.5,
                y: y as f32 + 0.5,
            });
            if inside != is_fill {
                mismatches.push((x, y));
            }
        }
    }
    ParityReport {
        checked,
        mismatches,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hittest::{Contour, Region};

    // A 4x4 region covering the left half (x < 2). Pixel centers at x=0.5,1.5
    // are inside; x=2.5,3.5 are outside.
    fn left_half_region() -> Region {
        Region {
            contours: vec![Contour {
                points: vec![
                    Point { x: 0.0, y: 0.0 },
                    Point { x: 2.0, y: 0.0 },
                    Point { x: 2.0, y: 4.0 },
                    Point { x: 0.0, y: 4.0 },
                ],
            }],
        }
    }

    fn solid_pixmap() -> Pixmap {
        // 4x4: left two columns red (fill), right two columns black (bg).
        let fill = [255u8, 0, 0, 255];
        let bg = [0u8, 0, 0, 255];
        let mut data = Vec::with_capacity(4 * 4 * 4);
        for _y in 0..4 {
            for x in 0..4 {
                let c = if x < 2 { fill } else { bg };
                data.extend_from_slice(&c);
            }
        }
        Pixmap {
            width: 4,
            height: 4,
            data,
        }
    }

    #[test]
    fn parity_passes_when_render_matches_hittest() {
        let report = parity_check(
            &left_half_region(),
            &solid_pixmap(),
            [255, 0, 0, 255],
            [0, 0, 0, 255],
        );
        assert_eq!(report.mismatches, Vec::<(u32, u32)>::new());
        assert_eq!(report.checked, 16); // no AA, so every pixel is checked
    }

    #[test]
    fn parity_catches_a_wrong_pixel() {
        let mut pm = solid_pixmap();
        // Corrupt pixel (3,0): paint it red though hittest says outside.
        let i = ((0 * 4 + 3) * 4) as usize;
        pm.data[i..i + 4].copy_from_slice(&[255, 0, 0, 255]);
        let report = parity_check(&left_half_region(), &pm, [255, 0, 0, 255], [0, 0, 0, 255]);
        assert_eq!(report.mismatches, vec![(3, 0)]);
    }

    #[test]
    fn parity_skips_antialiased_pixels() {
        let mut pm = solid_pixmap();
        // Blend pixel (3,0) to a non-fill, non-bg color: must be skipped, not a mismatch.
        let i = ((0 * 4 + 3) * 4) as usize;
        pm.data[i..i + 4].copy_from_slice(&[128, 0, 0, 255]);
        let report = parity_check(&left_half_region(), &pm, [255, 0, 0, 255], [0, 0, 0, 255]);
        assert_eq!(report.mismatches, Vec::<(u32, u32)>::new());
        assert_eq!(report.checked, 15); // the blended pixel was skipped
    }
}
