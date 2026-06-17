use mlua::{Function, Table};

use crate::scene::{Color, HandlerId, Node, Pt};

#[derive(Debug)]
pub enum BuildError {
    MissingField(&'static str),
    BadType(&'static str),
    Lua(mlua::Error),
}

impl From<mlua::Error> for BuildError {
    fn from(e: mlua::Error) -> Self {
        BuildError::Lua(e)
    }
}

/// Lets a primitive register a Lua handler (for hotspots) and receive a HandlerId.
pub trait BuildContext {
    fn register_handler(&mut self, f: Function) -> HandlerId;
}

/// A vocabulary entry a skin can construct: `id` is the Lua constructor name.
pub trait Primitive {
    fn id(&self) -> &str;
    fn build(&self, args: &Table, ctx: &mut dyn BuildContext) -> Result<Node, BuildError>;
}

pub fn parse_path(t: &Table) -> Result<Vec<Pt>, BuildError> {
    let path: Table = t.get("path").map_err(|_| BuildError::MissingField("path"))?;
    let mut pts = Vec::new();
    for entry in path.sequence_values::<Table>() {
        let p = entry?;
        pts.push(Pt { x: p.get("x")?, y: p.get("y")? });
    }
    if pts.len() < 3 {
        return Err(BuildError::BadType("path needs >= 3 points"));
    }
    Ok(pts)
}

pub fn parse_color(t: &Table) -> Result<Color, BuildError> {
    let c: Table = t.get("color").map_err(|_| BuildError::MissingField("color"))?;
    Ok(Color { r: c.get("r")?, g: c.get("g")?, b: c.get("b")? })
}

struct FillPrim;
impl Primitive for FillPrim {
    fn id(&self) -> &str {
        "fill"
    }
    fn build(&self, args: &Table, _ctx: &mut dyn BuildContext) -> Result<Node, BuildError> {
        Ok(Node::Fill { path: parse_path(args)?, color: parse_color(args)? })
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
        Ok(Node::Hotspot { region: crate::scene::region_of(&path), on_press: id })
    }
}

struct ValueFillPrim;
impl Primitive for ValueFillPrim {
    fn id(&self) -> &str {
        "value_fill"
    }
    fn build(&self, args: &Table, _ctx: &mut dyn BuildContext) -> Result<Node, BuildError> {
        let value_key: String = args.get("value").map_err(|_| BuildError::MissingField("value"))?;
        Ok(Node::ValueFill { path: parse_path(args)?, value_key, color: parse_color(args)? })
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
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mlua::Lua;

    struct NoHandlers;
    impl BuildContext for NoHandlers {
        fn register_handler(&mut self, _f: Function) -> HandlerId {
            0
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
            Node::Fill { color, path } => {
                assert_eq!(color, Color { r: 1, g: 2, b: 3 });
                assert_eq!(path.len(), 3);
            }
            other => panic!("expected Fill, got {other:?}"),
        }
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
        assert!(matches!(FillPrim.build(&t, &mut NoHandlers), Err(BuildError::MissingField("path"))));
    }

    #[test]
    fn base_registry_has_three() {
        assert_eq!(VocabRegistry::base().iter().count(), 3);
    }
}
