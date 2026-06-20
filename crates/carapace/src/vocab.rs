use std::sync::Arc;

use mlua::{Function, Table};

use crate::scene::{Color, ColorStop, Gradient, HandlerId, Node, Paint, Pt};

#[derive(Debug)]
pub enum BuildError {
    MissingField(&'static str),
    BadType(&'static str),
    Lua(mlua::Error),
    Asset(crate::asset::AssetError),
}

impl From<mlua::Error> for BuildError {
    fn from(e: mlua::Error) -> Self {
        BuildError::Lua(e)
    }
}

/// Lets a primitive register a Lua handler (for hotspots) and receive a HandlerId.
pub trait BuildContext {
    fn register_handler(&mut self, f: Function) -> HandlerId;
    fn image(
        &mut self,
        name: &str,
    ) -> Result<Arc<crate::asset::DecodedImage>, crate::asset::AssetError>;
    fn font(
        &mut self,
        name: &str,
    ) -> Result<Arc<crate::scene::FontData>, crate::asset::AssetError>;
}

/// A vocabulary entry a skin can construct: `id` is the Lua constructor name.
pub trait Primitive {
    fn id(&self) -> &str;
    fn build(&self, args: &Table, ctx: &mut dyn BuildContext) -> Result<Node, BuildError>;
}

pub fn parse_path(t: &Table) -> Result<Vec<Pt>, BuildError> {
    let path: Table = t
        .get("path")
        .map_err(|_| BuildError::MissingField("path"))?;
    let mut pts = Vec::new();
    for entry in path.sequence_values::<Table>() {
        let p = entry?;
        pts.push(Pt {
            x: p.get("x")?,
            y: p.get("y")?,
        });
    }
    if pts.len() < 3 {
        return Err(BuildError::BadType("path needs >= 3 points"));
    }
    Ok(pts)
}

/// Reads r,g,b from a color table; optional `a` defaults to 255 (opaque).
pub fn color_from_table(c: &Table) -> Result<Color, BuildError> {
    Ok(Color {
        r: c.get("r")?,
        g: c.get("g")?,
        b: c.get("b")?,
        a: c.get::<Option<u8>>("a")?.unwrap_or(255),
    })
}

pub fn parse_color(t: &Table) -> Result<Color, BuildError> {
    let c: Table = t
        .get("color")
        .map_err(|_| BuildError::MissingField("color"))?;
    color_from_table(&c)
}

fn parse_pt(t: &Table, key: &'static str) -> Result<Pt, BuildError> {
    let p: Table = t.get(key).map_err(|_| BuildError::MissingField(key))?;
    Ok(Pt {
        x: p.get("x")?,
        y: p.get("y")?,
    })
}

fn parse_stops(g: &Table) -> Result<Vec<ColorStop>, BuildError> {
    let stops_t: Table = g
        .get("stops")
        .map_err(|_| BuildError::MissingField("stops"))?;
    let mut stops = Vec::new();
    for entry in stops_t.sequence_values::<Table>() {
        let e = entry?;
        let at: f32 = e.get("at").map_err(|_| BuildError::MissingField("at"))?;
        let color_t: Table = e
            .get("color")
            .map_err(|_| BuildError::MissingField("color"))?;
        stops.push(ColorStop {
            at: at.clamp(0.0, 1.0),
            color: color_from_table(&color_t)?,
        });
    }
    if stops.len() < 2 {
        return Err(BuildError::BadType("gradient needs >= 2 stops"));
    }
    stops.sort_by(|a, b| a.at.partial_cmp(&b.at).unwrap_or(std::cmp::Ordering::Equal));
    Ok(stops)
}

fn parse_gradient(t: &Table) -> Result<Gradient, BuildError> {
    let g: Table = t
        .get("gradient")
        .map_err(|_| BuildError::MissingField("gradient"))?;
    let kind: String = g
        .get("type")
        .map_err(|_| BuildError::MissingField("type"))?;
    let stops = parse_stops(&g)?;
    Ok(match kind.as_str() {
        "linear" => Gradient::Linear {
            from: parse_pt(&g, "from")?,
            to: parse_pt(&g, "to")?,
            stops,
        },
        "radial" => Gradient::Radial {
            center: parse_pt(&g, "center")?,
            radius: g
                .get("radius")
                .map_err(|_| BuildError::MissingField("radius"))?,
            stops,
        },
        "sweep" => Gradient::Sweep {
            center: parse_pt(&g, "center")?,
            start_deg: g.get::<Option<f32>>("start_deg")?.unwrap_or(0.0),
            end_deg: g.get::<Option<f32>>("end_deg")?.unwrap_or(360.0),
            stops,
        },
        _ => {
            return Err(BuildError::BadType(
                "gradient type must be linear|radial|sweep",
            ));
        }
    })
}

fn parse_paint(args: &Table) -> Result<Paint, BuildError> {
    if args.contains_key("gradient")? {
        Ok(Paint::Gradient(parse_gradient(args)?))
    } else {
        Ok(Paint::Solid(parse_color(args)?))
    }
}

struct FillPrim;
impl Primitive for FillPrim {
    fn id(&self) -> &str {
        "fill"
    }
    fn build(&self, args: &Table, _ctx: &mut dyn BuildContext) -> Result<Node, BuildError> {
        Ok(Node::Fill {
            path: parse_path(args)?,
            paint: parse_paint(args)?,
        })
    }
}

struct RegionPrim;
impl Primitive for RegionPrim {
    fn id(&self) -> &str {
        "region"
    }
    fn build(&self, args: &Table, ctx: &mut dyn BuildContext) -> Result<Node, BuildError> {
        let path = parse_path(args)?;
        let on_press: Function = args
            .get("on_press")
            .map_err(|_| BuildError::MissingField("on_press"))?;
        let id = ctx.register_handler(on_press);
        Ok(Node::Hotspot {
            region: crate::scene::region_of(&path),
            on_press: id,
        })
    }
}

struct ValueFillPrim;
impl Primitive for ValueFillPrim {
    fn id(&self) -> &str {
        "value_fill"
    }
    fn build(&self, args: &Table, _ctx: &mut dyn BuildContext) -> Result<Node, BuildError> {
        let value_key: String = args
            .get("value")
            .map_err(|_| BuildError::MissingField("value"))?;
        Ok(Node::ValueFill {
            path: parse_path(args)?,
            value_key,
            color: parse_color(args)?,
        })
    }
}

struct ImagePrim;
impl Primitive for ImagePrim {
    fn id(&self) -> &str {
        "image"
    }
    fn build(&self, args: &Table, ctx: &mut dyn BuildContext) -> Result<Node, BuildError> {
        let name: String = args
            .get("asset")
            .map_err(|_| BuildError::MissingField("asset"))?;
        let image = ctx.image(&name).map_err(BuildError::Asset)?;
        let x: f32 = args.get("x").map_err(|_| BuildError::MissingField("x"))?;
        let y: f32 = args.get("y").map_err(|_| BuildError::MissingField("y"))?;
        let w: f32 = args.get("w").unwrap_or(image.width as f32);
        let h: f32 = args.get("h").unwrap_or(image.height as f32);
        Ok(Node::Image {
            image,
            dest: crate::scene::ImageDest { x, y, w, h },
        })
    }
}

pub struct VocabRegistry {
    prims: Vec<Box<dyn Primitive>>,
}

impl Default for VocabRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl VocabRegistry {
    pub fn new() -> Self {
        Self { prims: Vec::new() }
    }
    pub fn register(&mut self, prim: Box<dyn Primitive>) {
        self.prims.push(prim);
    }
    pub fn iter(&self) -> impl Iterator<Item = &dyn Primitive> {
        self.prims.iter().map(|b| b.as_ref())
    }
    /// The stub base set (Phase 5 replaces with the real vocabulary).
    pub fn base() -> Self {
        let mut r = Self::new();
        r.register(Box::new(FillPrim));
        r.register(Box::new(RegionPrim));
        r.register(Box::new(ValueFillPrim));
        r.register(Box::new(ImagePrim));
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{Gradient, Paint};
    use mlua::Lua;

    struct NoHandlers;
    impl BuildContext for NoHandlers {
        fn register_handler(&mut self, _f: Function) -> HandlerId {
            0
        }
        fn image(
            &mut self,
            name: &str,
        ) -> Result<std::sync::Arc<crate::asset::DecodedImage>, crate::asset::AssetError> {
            Err(crate::asset::AssetError::Unresolved(name.to_string()))
        }
        fn font(
            &mut self,
            name: &str,
        ) -> Result<std::sync::Arc<crate::scene::FontData>, crate::asset::AssetError> {
            Err(crate::asset::AssetError::Unresolved(name.to_string()))
        }
    }

    fn tbl(lua: &Lua, src: &str) -> Table {
        lua.load(src).eval().unwrap()
    }

    #[test]
    fn fill_builds_fill_node() {
        let lua = Lua::new();
        let t = tbl(
            &lua,
            "return { path = {{x=0,y=0},{x=10,y=0},{x=10,y=10}}, color = {r=1,g=2,b=3} }",
        );
        let node = FillPrim.build(&t, &mut NoHandlers).unwrap();
        match node {
            Node::Fill { paint, path } => {
                assert_eq!(
                    paint,
                    Paint::Solid(Color {
                        r: 1,
                        g: 2,
                        b: 3,
                        a: 255
                    })
                );
                assert_eq!(path.len(), 3);
            }
            other => panic!("expected Fill, got {other:?}"),
        }
    }

    #[test]
    fn color_alpha_defaults_opaque_and_parses_explicit() {
        let lua = Lua::new();
        let opaque: Table = lua.load("return { color = {r=1,g=2,b=3} }").eval().unwrap();
        assert_eq!(
            parse_color(&opaque).unwrap(),
            Color {
                r: 1,
                g: 2,
                b: 3,
                a: 255
            }
        );
        let translucent: Table = lua
            .load("return { color = {r=1,g=2,b=3,a=90} }")
            .eval()
            .unwrap();
        assert_eq!(
            parse_color(&translucent).unwrap(),
            Color {
                r: 1,
                g: 2,
                b: 3,
                a: 90
            }
        );
    }

    #[test]
    fn value_fill_keeps_binding_key() {
        let lua = Lua::new();
        let t = tbl(
            &lua,
            "return { path = {{x=0,y=0},{x=1,y=0},{x=1,y=1}}, value = 'level', color = {r=0,g=0,b=0} }",
        );
        match ValueFillPrim.build(&t, &mut NoHandlers).unwrap() {
            Node::ValueFill { value_key, .. } => assert_eq!(value_key, "level"),
            other => panic!("expected ValueFill, got {other:?}"),
        }
    }

    #[test]
    fn region_registers_handler_and_caches_region() {
        struct Counter(HandlerId);
        impl BuildContext for Counter {
            fn register_handler(&mut self, _f: Function) -> HandlerId {
                let id = self.0;
                self.0 += 1;
                id
            }
            fn image(
                &mut self,
                name: &str,
            ) -> Result<std::sync::Arc<crate::asset::DecodedImage>, crate::asset::AssetError>
            {
                Err(crate::asset::AssetError::Unresolved(name.to_string()))
            }
            fn font(
                &mut self,
                name: &str,
            ) -> Result<std::sync::Arc<crate::scene::FontData>, crate::asset::AssetError>
            {
                Err(crate::asset::AssetError::Unresolved(name.to_string()))
            }
        }
        let lua = Lua::new();
        let t = tbl(
            &lua,
            "return { path = {{x=0,y=0},{x=1,y=0},{x=1,y=1}}, on_press = function() end }",
        );
        let mut ctx = Counter(5);
        match RegionPrim.build(&t, &mut ctx).unwrap() {
            Node::Hotspot { on_press, .. } => assert_eq!(on_press, 5),
            other => panic!("expected Hotspot, got {other:?}"),
        }
        assert_eq!(ctx.0, 6, "handler id was allocated");
    }

    #[test]
    fn missing_field_errors() {
        let lua = Lua::new();
        let t = tbl(&lua, "return { color = {r=0,g=0,b=0} }"); // no path
        assert!(matches!(
            FillPrim.build(&t, &mut NoHandlers),
            Err(BuildError::MissingField("path"))
        ));
    }

    #[test]
    fn base_registry_now_has_four() {
        assert_eq!(VocabRegistry::base().iter().count(), 4);
    }

    #[test]
    fn fill_builds_linear_gradient() {
        let lua = Lua::new();
        let t = tbl(
            &lua,
            "return { path = {{x=0,y=0},{x=10,y=0},{x=10,y=10}}, gradient = { \
               type='linear', from={x=0,y=0}, to={x=0,y=40}, \
               stops = { {at=1, color={r=9,g=9,b=9,a=0}}, {at=0, color={r=255,g=255,b=255,a=120}} } } }",
        );
        match FillPrim.build(&t, &mut NoHandlers).unwrap() {
            Node::Fill {
                paint: Paint::Gradient(Gradient::Linear { from, to, stops }),
                ..
            } => {
                assert_eq!((from, to), (Pt { x: 0.0, y: 0.0 }, Pt { x: 0.0, y: 40.0 }));
                // stops sorted by `at`
                assert_eq!(stops.len(), 2);
                assert_eq!(stops[0].at, 0.0);
                assert_eq!(
                    stops[0].color,
                    Color {
                        r: 255,
                        g: 255,
                        b: 255,
                        a: 120
                    }
                );
                assert_eq!(stops[1].at, 1.0);
            }
            other => panic!("expected linear gradient fill, got {other:?}"),
        }
    }

    #[test]
    fn radial_and_sweep_parse_with_defaults() {
        let lua = Lua::new();
        let radial = tbl(
            &lua,
            "return { path = {{x=0,y=0},{x=1,y=0},{x=1,y=1}}, gradient = { \
               type='radial', center={x=5,y=6}, radius=7, \
               stops = { {at=0, color={r=0,g=0,b=0}}, {at=1, color={r=1,g=1,b=1}} } } }",
        );
        match FillPrim.build(&radial, &mut NoHandlers).unwrap() {
            Node::Fill {
                paint: Paint::Gradient(Gradient::Radial { center, radius, .. }),
                ..
            } => {
                assert_eq!((center, radius), (Pt { x: 5.0, y: 6.0 }, 7.0));
            }
            other => panic!("expected radial, got {other:?}"),
        }
        // sweep with default angles 0..360
        let sweep = tbl(
            &lua,
            "return { path = {{x=0,y=0},{x=1,y=0},{x=1,y=1}}, gradient = { \
               type='sweep', center={x=2,y=3}, \
               stops = { {at=0, color={r=0,g=0,b=0}}, {at=1, color={r=1,g=1,b=1}} } } }",
        );
        match FillPrim.build(&sweep, &mut NoHandlers).unwrap() {
            Node::Fill {
                paint:
                    Paint::Gradient(Gradient::Sweep {
                        start_deg, end_deg, ..
                    }),
                ..
            } => {
                assert_eq!((start_deg, end_deg), (0.0, 360.0));
            }
            other => panic!("expected sweep, got {other:?}"),
        }
    }

    #[test]
    fn gradient_rejects_bad_type_and_too_few_stops() {
        let lua = Lua::new();
        let bad_type = tbl(
            &lua,
            "return { path = {{x=0,y=0},{x=1,y=0},{x=1,y=1}}, gradient = { \
               type='conic', from={x=0,y=0}, to={x=1,y=1}, \
               stops = { {at=0, color={r=0,g=0,b=0}}, {at=1, color={r=1,g=1,b=1}} } } }",
        );
        assert!(matches!(
            FillPrim.build(&bad_type, &mut NoHandlers),
            Err(BuildError::BadType(_))
        ));
        let one_stop = tbl(
            &lua,
            "return { path = {{x=0,y=0},{x=1,y=0},{x=1,y=1}}, gradient = { \
               type='linear', from={x=0,y=0}, to={x=1,y=1}, \
               stops = { {at=0, color={r=0,g=0,b=0}} } } }",
        );
        assert!(matches!(
            FillPrim.build(&one_stop, &mut NoHandlers),
            Err(BuildError::BadType(_))
        ));
    }

    #[test]
    fn image_prim_builds_native_and_scaled() {
        use crate::asset::DecodedImage;
        use std::sync::Arc;
        struct Ctx(Arc<DecodedImage>);
        impl BuildContext for Ctx {
            fn register_handler(&mut self, _f: Function) -> HandlerId {
                0
            }
            fn image(
                &mut self,
                _name: &str,
            ) -> Result<Arc<DecodedImage>, crate::asset::AssetError> {
                Ok(self.0.clone())
            }
            fn font(
                &mut self,
                name: &str,
            ) -> Result<std::sync::Arc<crate::scene::FontData>, crate::asset::AssetError> {
                Err(crate::asset::AssetError::Unresolved(name.to_string()))
            }
        }
        let img = Arc::new(DecodedImage {
            rgba: vec![0; 16],
            width: 4,
            height: 2,
        });
        let lua = mlua::Lua::new();
        // native: dest = (x,y, native w,h)
        let t: Table = lua
            .load("return { asset='a.png', x=10, y=20 }")
            .eval()
            .unwrap();
        match ImagePrim.build(&t, &mut Ctx(img.clone())).unwrap() {
            Node::Image { dest, .. } => {
                assert_eq!((dest.x, dest.y, dest.w, dest.h), (10.0, 20.0, 4.0, 2.0));
            }
            other => panic!("expected Image, got {other:?}"),
        }
        // scaled: explicit w,h
        let t2: Table = lua
            .load("return { asset='a.png', x=0, y=0, w=40, h=30 }")
            .eval()
            .unwrap();
        match ImagePrim.build(&t2, &mut Ctx(img)).unwrap() {
            Node::Image { dest, .. } => assert_eq!((dest.w, dest.h), (40.0, 30.0)),
            other => panic!("expected Image, got {other:?}"),
        }
    }
}
