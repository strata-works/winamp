//! The `shader{}` primitive: a GPU fragment-shader fill node, parsed from Lua and naga-validated
//! entirely at skin-load time (no GPU adapter required). See `Node::Shader` for the produced node
//! shape and `render.rs`/the renderer's 4-stage compositing path for how it's drawn.

use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

use mlua::{Table, Value as LuaValue};

use crate::scene::{ImageDest, Node};
use crate::vocab::{BuildContext, BuildError, Primitive};

/// One `shader{}` uniform binding: a named `f32` field in the generated `struct U` (see
/// [`prelude_for`]), plus where its value comes from each frame.
#[derive(Clone, Debug, PartialEq)]
pub struct ShaderUniform {
    /// The uniform's field name, as it appears in the generated `struct U` and the Lua
    /// `uniforms` table key.
    pub name: String,
    /// Where this uniform's value comes from.
    pub source: UniformSource,
}

/// Where a [`ShaderUniform`]'s per-frame value comes from.
#[derive(Clone, Debug, PartialEq)]
pub enum UniformSource {
    /// A fixed value baked in at skin-load time (author wrote a Lua number).
    Literal(f32),
    /// A value read from host state at the given key, each frame (author wrote a Lua string).
    Host(String),
}

/// Emit the fixed WGSL prelude (the fullscreen-quad vertex stage every `shader{}` reuses,
/// verbatim from `shader_prelude.wgsl`) plus a generated `struct U` with the built-in `time`/
/// `res` fields and one named `f32` field per entry of `names`, and the `@group(0) @binding(0)`
/// uniform-buffer binding the author's fragment stage reads from.
///
/// Fields are named `f32`s rather than `array<f32, N>` deliberately: WGSL's uniform-address-space
/// array element stride is 16 bytes (each `f32` would burn a full vec4 slot), while named `f32`
/// fields pack tightly under the uniform layout rules instead.
pub fn prelude_for(names: &[&str]) -> String {
    let mut s = String::new();
    s.push_str(include_str!("shader_prelude.wgsl"));
    s.push_str("\nstruct U {\n    time: f32,\n    res: vec2<f32>,\n");
    for name in names {
        s.push_str(&format!("    {name}: f32,\n"));
    }
    s.push_str("};\n@group(0) @binding(0) var<uniform> u: U;\n");
    s
}

/// The `shader{ src, x, y, w, h, uniforms }` primitive: resolves the author's WGSL fragment
/// source, combines it with the generated prelude, naga-validates the result at load time, and
/// emits a single [`Node::Shader`].
pub struct ShaderPrim;

impl Primitive for ShaderPrim {
    fn id(&self) -> &str {
        "shader"
    }

    fn build(&self, args: &Table, ctx: &mut dyn BuildContext) -> Result<Vec<Node>, BuildError> {
        let src: String = args
            .get("src")
            .map_err(|_| BuildError::MissingField("src"))?;
        let x: f32 = args.get("x").map_err(|_| BuildError::MissingField("x"))?;
        let y: f32 = args.get("y").map_err(|_| BuildError::MissingField("y"))?;
        let w: f32 = args.get("w").map_err(|_| BuildError::MissingField("w"))?;
        let h: f32 = args.get("h").map_err(|_| BuildError::MissingField("h"))?;

        let mut uniforms: Vec<ShaderUniform> = Vec::new();
        if let Some(t) = args.get::<Option<Table>>("uniforms")? {
            for pair in t.pairs::<String, LuaValue>() {
                let (name, v) = pair?;
                let source = match v {
                    LuaValue::Integer(i) => UniformSource::Literal(i as f32),
                    LuaValue::Number(n) => UniformSource::Literal(n as f32),
                    LuaValue::String(s) => UniformSource::Host(s.to_str()?.to_string()),
                    _ => {
                        return Err(BuildError::BadType(
                            "uniforms values must be a number or string",
                        ));
                    }
                };
                uniforms.push(ShaderUniform { name, source });
            }
        }

        let author = ctx.shader_src(&src).map_err(BuildError::Asset)?;
        let names: Vec<&str> = uniforms.iter().map(|u| u.name.as_str()).collect();
        let full = format!("{}\n{}", prelude_for(&names), author);

        naga::front::wgsl::parse_str(&full)
            .map_err(|e| BuildError::Shader(format!("shader {src}: {e}")))?;

        let mut hasher = DefaultHasher::new();
        full.hash(&mut hasher);
        let key = hasher.finish();

        Ok(vec![Node::Shader {
            dest: ImageDest { x, y, w, h },
            wgsl: Arc::from(full.as_str()),
            uniforms,
            key,
        }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mlua::{Function, Lua};

    /// A minimal `BuildContext` test double: no handlers/assets, `shader_src` returns a
    /// trivial valid fragment stage (pairs with the fixed vertex stage in `shader_prelude.wgsl`).
    struct NoHandlers;
    impl BuildContext for NoHandlers {
        fn register_handler(&mut self, _f: Function) -> crate::scene::HandlerId {
            0
        }
        fn host_action(
            &mut self,
            _action: &str,
            _args: Vec<crate::host::Value>,
        ) -> crate::scene::HandlerId {
            0
        }
        fn image(
            &mut self,
            name: &str,
        ) -> Result<Arc<crate::asset::DecodedImage>, crate::asset::AssetError> {
            Err(crate::asset::AssetError::Unresolved(name.to_string()))
        }
        fn font(
            &mut self,
            name: &str,
        ) -> Result<Arc<crate::scene::FontData>, crate::asset::AssetError> {
            Err(crate::asset::AssetError::Unresolved(name.to_string()))
        }
        fn shader_src(&mut self, _name: &str) -> Result<Arc<str>, crate::asset::AssetError> {
            Ok(Arc::from(
                "@fragment fn fs(in: VsOut) -> @location(0) vec4<f32> { return vec4(0.0); }",
            ))
        }
    }

    /// A `BuildContext` test double whose `shader_src` returns malformed WGSL, for the
    /// naga-validation-failure test.
    struct BadWgsl;
    impl BuildContext for BadWgsl {
        fn register_handler(&mut self, _f: Function) -> crate::scene::HandlerId {
            0
        }
        fn host_action(
            &mut self,
            _action: &str,
            _args: Vec<crate::host::Value>,
        ) -> crate::scene::HandlerId {
            0
        }
        fn image(
            &mut self,
            name: &str,
        ) -> Result<Arc<crate::asset::DecodedImage>, crate::asset::AssetError> {
            Err(crate::asset::AssetError::Unresolved(name.to_string()))
        }
        fn font(
            &mut self,
            name: &str,
        ) -> Result<Arc<crate::scene::FontData>, crate::asset::AssetError> {
            Err(crate::asset::AssetError::Unresolved(name.to_string()))
        }
        fn shader_src(&mut self, _name: &str) -> Result<Arc<str>, crate::asset::AssetError> {
            Ok(Arc::from("this is not wgsl {{{"))
        }
    }

    fn tbl(lua: &Lua, src: &str) -> Table {
        lua.load(src).eval().unwrap()
    }

    #[test]
    fn shader_prim_parses_dest_and_uniforms() {
        let lua = Lua::new();
        let t = tbl(
            &lua,
            "return { src='x.wgsl', x=0, y=0, w=480, h=320, uniforms = { season=2, temp='wx_temp' } }",
        );
        let nodes = ShaderPrim.build(&t, &mut NoHandlers).unwrap();
        assert_eq!(nodes.len(), 1);
        let Node::Shader { dest, uniforms, .. } = &nodes[0] else {
            panic!("expected Node::Shader, got {:?}", nodes[0]);
        };
        assert_eq!((dest.x, dest.y, dest.w, dest.h), (0.0, 0.0, 480.0, 320.0));
        assert!(uniforms.iter().any(
            |u| u.name == "season" && matches!(u.source, UniformSource::Literal(v) if v == 2.0)
        ));
        assert!(
            uniforms.iter().any(|u| u.name == "temp"
                && matches!(&u.source, UniformSource::Host(k) if k == "wx_temp"))
        );
    }

    #[test]
    fn shader_prim_rejects_malformed_wgsl() {
        let lua = Lua::new();
        let t = tbl(&lua, "return { src='bad.wgsl', x=0, y=0, w=10, h=10 }");
        let err = ShaderPrim.build(&t, &mut BadWgsl).unwrap_err();
        assert!(matches!(err, BuildError::Shader(_)));
    }

    #[test]
    fn generated_prelude_declares_named_uniform_fields() {
        let s = prelude_for(&["season", "temp"]);
        assert!(s.contains("struct U"));
        assert!(s.contains("time: f32"));
        assert!(s.contains("res: vec2<f32>"));
        assert!(s.contains("season: f32"));
        assert!(s.contains("temp: f32"));
        assert!(s.contains("@group(0) @binding(0) var<uniform> u: U;"));
    }

    #[test]
    fn uniform_referencing_fragment_validates() {
        let prelude = prelude_for(&["season", "temp"]);
        let fragment = "@fragment fn fs(in: VsOut) -> @location(0) vec4<f32> { \
            return vec4(u.season, u.temp, u.time, 1.0); }";
        let full = format!("{prelude}\n{fragment}");
        assert!(
            naga::front::wgsl::parse_str(&full).is_ok(),
            "prelude + uniform-referencing fragment should validate cleanly"
        );
    }
}
