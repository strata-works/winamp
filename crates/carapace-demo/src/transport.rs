//! A host extension: the media transport, registered by the demo host. Defined in this
//! external crate (not the engine) — it implements `carapace::vocab::Primitive` from
//! carapace's public API alone, binding the host's own actions via `host_action`.

use carapace::mlua::Table;
use carapace::scene::{Color, FillDir, Node, Paint, region_of};
use carapace::shape;
use carapace::vocab::{BuildContext, BuildError, Primitive};

pub struct TransportPrim;

fn solid(r: u8, g: u8, b: u8) -> Paint {
    Paint::Solid(Color { r, g, b, a: 255 })
}

impl Primitive for TransportPrim {
    fn id(&self) -> &str {
        "transport"
    }

    fn build(&self, args: &Table, ctx: &mut dyn BuildContext) -> Result<Vec<Node>, BuildError> {
        let x: f32 = args.get("x").map_err(|_| BuildError::MissingField("x"))?;
        let y: f32 = args.get("y").map_err(|_| BuildError::MissingField("y"))?;

        let play = shape::rect(x, y, 40.0, 40.0);
        let play_id = ctx.host_action("toggle_play", vec![]);
        let stop = shape::rect(x + 48.0, y, 40.0, 40.0);
        let stop_id = ctx.host_action("stop", vec![]);
        let seek = shape::rect(x, y + 48.0, 88.0, 10.0);

        Ok(vec![
            Node::Fill {
                path: play.clone(),
                paint: solid(80, 200, 120),
            },
            Node::Hotspot {
                region: region_of(&play),
                on_press: play_id,
            },
            Node::Fill {
                path: stop.clone(),
                paint: solid(200, 80, 80),
            },
            Node::Hotspot {
                region: region_of(&stop),
                on_press: stop_id,
            },
            Node::ValueFill {
                path: seek,
                value_key: "position".to_string(),
                color: Color {
                    r: 240,
                    g: 220,
                    b: 80,
                    a: 255,
                },
                direction: FillDir::Right,
            },
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use carapace::vocab::VocabRegistry;

    #[test]
    fn registers_into_a_vocab_registry() {
        // The seam: an external crate's primitive registers like a built-in (base 5 + this = 6).
        let mut reg = VocabRegistry::base();
        reg.register(Box::new(TransportPrim));
        assert_eq!(reg.iter().count(), 6);
    }
}
