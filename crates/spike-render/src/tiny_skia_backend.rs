use crate::{Pixmap, Renderer};
use hittest::Region;
use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap as TsPixmap, Transform};

pub struct TinySkiaRenderer;

impl Renderer for TinySkiaRenderer {
    fn name(&self) -> &'static str {
        "tiny-skia"
    }

    fn render(&mut self, region: &Region, size: (u32, u32), fill: [u8; 4], bg: [u8; 4]) -> Pixmap {
        let (w, h) = size;
        let mut pm = TsPixmap::new(w, h).expect("valid pixmap size");
        pm.fill(Color::from_rgba8(bg[0], bg[1], bg[2], bg[3]));

        let mut pb = PathBuilder::new();
        for contour in &region.contours {
            if let Some((first, rest)) = contour.points.split_first() {
                pb.move_to(first.x, first.y);
                for p in rest {
                    pb.line_to(p.x, p.y);
                }
                pb.close();
            }
        }

        if let Some(path) = pb.finish() {
            let mut paint = Paint::default();
            paint.set_color(Color::from_rgba8(fill[0], fill[1], fill[2], fill[3]));
            paint.anti_alias = true;
            pm.fill_path(&path, &paint, FillRule::EvenOdd, Transform::identity(), None);
        }

        // tiny-skia stores premultiplied RGBA8; fill/bg here are opaque so the
        // stored bytes equal the input colors exactly for solid pixels.
        Pixmap {
            width: w,
            height: h,
            data: pm.data().to_vec(),
        }
    }
}
