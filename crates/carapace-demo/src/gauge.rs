//! A system-monitor domain extension: a labeled vertical meter. Defined in the demo crate,
//! registered by the host — composes the base vocab (a vertical value_fill + a text label + a
//! frame) entirely from carapace's public API.

use carapace::mlua::Table;
use carapace::scene::{Color, FillDir, HAlign, Node, Paint, Pt, TextContent, VAlign};
use carapace::shape;
use carapace::vocab::{BuildContext, BuildError, Primitive};

pub struct GaugePrim;

fn solid(r: u8, g: u8, b: u8) -> Paint {
    Paint::Solid(Color { r, g, b, a: 255 })
}

impl Primitive for GaugePrim {
    fn id(&self) -> &str {
        "gauge"
    }
    fn build(&self, args: &Table, _ctx: &mut dyn BuildContext) -> Result<Vec<Node>, BuildError> {
        let x: f32 = args.get("x").map_err(|_| BuildError::MissingField("x"))?;
        let y: f32 = args.get("y").map_err(|_| BuildError::MissingField("y"))?;
        let value: String = args
            .get("value")
            .map_err(|_| BuildError::MissingField("value"))?;
        let label: String = args
            .get("label")
            .map_err(|_| BuildError::MissingField("label"))?;

        let frame = shape::rounded_rect(x, y, 40.0, 100.0, 6.0, 6);
        let bar = shape::rect(x + 6.0, y + 6.0, 28.0, 88.0);
        Ok(vec![
            Node::Fill {
                path: frame,
                paint: solid(30, 36, 48),
            },
            Node::ValueFill {
                path: bar,
                value_key: value,
                color: Color {
                    r: 90,
                    g: 210,
                    b: 160,
                    a: 255,
                },
                direction: FillDir::Up,
            },
            Node::Text {
                content: TextContent::Static(label),
                font: None,
                font_name: None,
                size: 12.0,
                paint: solid(210, 220, 230),
                halign: HAlign::Center,
                valign: VAlign::Top,
                max_width: None,
                pos: Pt {
                    x: x + 20.0,
                    y: y + 104.0,
                },
            },
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use carapace::scene::{FillDir, Node};
    use carapace::vocab::VocabRegistry;

    #[test]
    fn registers_and_builds_a_vertical_gauge_with_label() {
        let mut reg = VocabRegistry::base();
        reg.register(Box::new(GaugePrim));
        assert_eq!(reg.iter().count(), 7); // base 6 + gauge

        let lua = carapace::mlua::Lua::new();
        let t: carapace::mlua::Table = lua
            .load("return { x=10, y=10, value='cpu', label='CPU' }")
            .eval()
            .unwrap();
        struct NoCtx;
        impl carapace::vocab::BuildContext for NoCtx {
            fn register_handler(&mut self, _f: carapace::mlua::Function) -> usize {
                0
            }
            fn host_action(&mut self, _a: &str, _args: Vec<carapace::host::Value>) -> usize {
                0
            }
            fn image(
                &mut self,
                n: &str,
            ) -> Result<std::sync::Arc<carapace::asset::DecodedImage>, carapace::asset::AssetError>
            {
                Err(carapace::asset::AssetError::Unresolved(n.to_string()))
            }
            fn font(
                &mut self,
                n: &str,
            ) -> Result<std::sync::Arc<carapace::scene::FontData>, carapace::asset::AssetError>
            {
                Err(carapace::asset::AssetError::Unresolved(n.to_string()))
            }
        }
        let nodes = GaugePrim.build(&t, &mut NoCtx).unwrap();
        assert!(nodes.iter().any(|n| matches!(
            n,
            Node::ValueFill {
                direction: FillDir::Up,
                ..
            }
        )));
        assert!(nodes.iter().any(|n| matches!(n, Node::Text { .. })));
    }
}
