# carapace-preview: Inspector + Parameters + Write-Back Implementation Plan (Plan B)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the design's editing half to `carapace-preview`: a **parameters panel** (edit top-level `local NAME = literal` values) and a **property inspector** (click a rendered node → edit its literal props), both **rewriting `skin.lua` on disk** so the watcher reloads and re-renders. Read-only props show a reason.

**Architecture:** Provenance is now **two correlated sources** that are both reachable: (1) the engine stamps every scene node with its source **line + call ordinal** (`Engine::layout_with_origins`, from the `scene-node-provenance` change), and (2) this crate **statically parses `skin.lua` with `full_moon`** to index each primitive call's **literal fields** and each top-level **`local` literal** with exact byte spans. A picked node's origin line selects its source call; write-back splices the exact literal span and the file watcher does the rest. The runtime mlua-hook approach in the original design is abandoned as infeasible from a downstream crate — see "Design corrections" below.

**Tech Stack:** Rust (edition 2024), existing `carapace-preview` deps, **new:** `full_moon` (Lua AST + source positions). Depends on the `carapace` engine API added by `docs/superpowers/plans/2026-07-03-scene-node-provenance.md` (`layout_with_origins`, `scene::Origin`, `Scene::pick`) — **that plan must land first.**

Design specs: `docs/superpowers/specs/2026-07-01-carapace-preview-design.md` (editing section) and `docs/superpowers/specs/2026-07-03-scene-node-provenance-design.md`.

## Design corrections vs the 2026-07-01 spec

- **Runtime provenance is dropped.** The spec assumed an mlua debug hook could tag nodes with source spans from outside the engine. A 2026-07-03 code survey proved `mlua::Lua` is private and the `debug` lib is sandboxed out — impossible from this crate. Provenance now comes from the engine's new per-node `Origin { line, call }` (captured inside the engine at load) **correlated** to a static `full_moon` parse for exact byte spans.
- **Node identity = source line + call ordinal**, not a runtime span. A source call that runs once → editable; a call that runs many times (loop) → the same line appears under multiple `call` ordinals → **read-only ("from a loop")**. A `fill{}` with `on_press` emits Fill+Hotspot sharing one `call` → still one editable primitive.
- **Text nodes are not pickable** (no measured geometry in the scene); text props are reachable via parameters, not the click path. Documented limitation.
- **Two primitives on the same physical source line** are indistinguishable (runtime gives line only). Skins place one primitive per line; documented.

## Global Constraints

- **Edition:** `edition = "2024"`. **Zero engine-crate diff in THIS plan** — all engine changes are in the prerequisite `scene-node-provenance` plan; here we only consume the new public API.
- **Dependency fetch policy:** the first fetch of `full_moon` must go through Socket Firewall — `sfw cargo add -p carapace-preview full_moon --features lua54` (never a bare `cargo add`). No other new deps.
- **Write-back is a splice, never a regeneration.** Every edit replaces the exact byte range of one literal in `skin.lua`; comments/formatting elsewhere are untouched. On every write-back, **re-parse the current file** (it may have changed since the panel was populated) before locating the span.
- **Engine is single-threaded / `!Send`.** New engine-side work (pick, layout_with_origins, source parse) runs on the main/engine thread. Server/watcher threads still exchange only `Send` data (the new protocol messages carry numbers/strings/JSON).
- **Commit after every task.** Repo git identity (Daniel Agbemava <danagbemava@gmail.com>) is the default.
- **Before finishing:** `cargo clippy -p carapace-preview --all-targets -- -D warnings` and `cargo fmt` clean (CI gates on clippy `-D warnings`). Keep the README current (repo convention: refresh in the same PR as the feature).

## Verified facts this plan is built on

- **Engine API (from the prerequisite plan):** `Engine::layout_with_origins(&self, w: f32, h: f32) -> (Scene, Vec<carapace::scene::Origin>)`; `Origin { pub line: Option<u32>, pub call: Option<u32> }`; `Scene::pick(&self, p: Pt) -> Option<usize>` (topmost node index by bbox, skips zero-area/text); `carapace::layout::node_bbox(&Node) -> Option<Rect>`.
- **full_moon** (`docs.rs/full_moon`, 1.x): `full_moon::parse(code: &str) -> Result<Ast, full_moon::Error>`; `Ast::nodes() -> &Block`; `Block::stmts() -> impl Iterator<Item = &Stmt>`. `Stmt::LocalAssignment(LocalAssignment)` and `Stmt::FunctionCall(FunctionCall)` are the two we handle. `LocalAssignment::names() -> &Punctuated<TokenReference>`, `::expressions() -> &Punctuated<Expression>`. The `Node` trait gives `range() -> Option<(Position, Position)>` and `start_position()`; `Position::bytes() -> usize` / `line() -> usize`. Exact `Expression` / `Field` / `FunctionArgs` variant names are pinned by the characterization test in Task 1 Step 1 (adjust to the resolved 1.x if a variant name differs — the TDD loop catches it).
- **Current crate (post-#30):** `protocol.rs` — `ClientMsg` (`serde(tag="type", rename_all="camelCase")`) with `Pointer/SetValue/RemoveValue/SetCanvas`; `OutMsg` with `Frame/Meta/ActionLog/Error`; `out_to_ws` maps each. `main.rs` — `run_engine_loop` matches `EngineMsg::Client(ClientMsg::…)`; `SkinSession` holds `engine: Option<Engine>`, `dir`, `canvas`, `last_error`. `skin_session.rs::build_engine` calls `load_dir` and reads `source.lua_src`. `assets/index.html` — single-file viewer; `render_index` templates `{{WS_PORT}}`.
- `SkinSource.lua_src: pub String`, `Manifest.entry: pub String` (`carapace::skin::load_dir`). The on-disk file is `dir.join(&manifest.entry)`.

## File Structure

```
crates/carapace-preview/
  Cargo.toml            # + full_moon
  src/
    provenance.rs       # NEW: full_moon parse → SourceModel (params + call literal fields, byte spans) + splice write-back
    inspector.rs        # NEW: correlate engine origins + picked node index + SourceModel → NodeInfo (editable/read-only + reason)
    protocol.rs         # + Pick/SetProp/SetParam (up), NodeInfo/Params (down)
    skin_session.rs     # + source_model(), pick_node(), apply_prop()/apply_param() write-to-disk, entry path
    main.rs             # + handle Pick/SetProp/SetParam; broadcast Params on connect/reload
    assets/index.html   # + inspector panel + parameters panel + "Inspect" toggle
```

Provenance model types (shared vocabulary for the tasks below):

```rust
// provenance.rs
pub struct SourceModel {
    pub params: Vec<Param>,      // top-level `local NAME = <literal>`
    pub calls: Vec<CallSite>,    // top-level primitive calls, in source order
}
pub struct Param { pub name: String, pub value: LiteralValue }
pub struct CallSite { pub line: u32, pub prim: String, pub fields: Vec<FieldInfo> }
pub struct FieldInfo { pub name: String, pub state: FieldState }
pub enum FieldState {
    Literal { value: LiteralValue },     // editable
    NonLiteral { reason: String },       // read-only (bound/computed/expression)
}
pub enum LiteralValue {
    Scalar { text: String, span: (usize, usize) },              // number/string/bool literal
    Table  { subfields: Vec<(String, ScalarSpan)> },            // e.g. color {r,g,b}
}
pub struct ScalarSpan { pub text: String, pub span: (usize, usize) }
```

---

### Task 1: Add `full_moon` + `provenance.rs` parameters extraction + splice

**Files:**
- Modify: `crates/carapace-preview/Cargo.toml`
- Create: `crates/carapace-preview/src/provenance.rs`
- Modify: `crates/carapace-preview/src/main.rs` (add `mod provenance;`)

**Interfaces:**
- Produces: `provenance::parse_params(src: &str) -> Vec<Param>` (top-level `local NAME = <scalar literal>`); `provenance::splice(src: &str, span: (usize, usize), new_text: &str) -> String`; the `Param`/`LiteralValue`/`ScalarSpan` types above (scalar arm first).

- [ ] **Step 1: Add the dependency through Socket Firewall**

Run from repo root:
```bash
sfw cargo add -p carapace-preview full_moon --features lua54
```
Expected: `full_moon` appended to `crates/carapace-preview/Cargo.toml`. Then `cargo build -p carapace-preview` resolves it.

- [ ] **Step 2: Write the failing tests (characterization + behavior)**

Create `crates/carapace-preview/src/provenance.rs` with tests first. The first test also **pins the full_moon shape** we depend on:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_moon_parses_and_ranges_slice_the_source() {
        // Characterization: prove parse + Node::range byte offsets slice the exact literal.
        use full_moon::node::Node;
        let src = "local RI = 90\n";
        let ast = full_moon::parse(src).expect("parse");
        let stmt = ast.nodes().stmts().next().expect("one stmt");
        let full_moon::ast::Stmt::LocalAssignment(la) = stmt else { panic!("expected local") };
        let expr = la.expressions().iter().next().expect("one expr");
        let (a, b) = expr.range().expect("range");
        assert_eq!(&src[a.bytes()..b.bytes()], "90", "range slices the literal exactly");
    }

    #[test]
    fn extracts_scalar_params_with_spans() {
        let src = "local RI = 90\nlocal NAME = \"door\"\nlocal ON = true\n";
        let params = parse_params(src);
        let by = |n: &str| params.iter().find(|p| p.name == n).cloned().unwrap();
        match by("RI").value { LiteralValue::Scalar { text, span } => {
            assert_eq!(text, "90");
            assert_eq!(&src[span.0..span.1], "90");
        } _ => panic!("scalar") }
        match by("NAME").value { LiteralValue::Scalar { text, .. } => assert_eq!(text, "\"door\""), _ => panic!() }
        match by("ON").value { LiteralValue::Scalar { text, .. } => assert_eq!(text, "true"), _ => panic!() }
    }

    #[test]
    fn skips_non_literal_and_multi_name_locals() {
        // Expressions (RI+10), function calls, and `local a, b = ...` are not scalar params.
        let src = "local A = RI + 10\nlocal B = circle{cx=1}\nlocal C, D = 1, 2\n";
        assert!(parse_params(src).is_empty());
    }

    #[test]
    fn splice_replaces_only_the_span() {
        let src = "local RI = 90\n";
        let out = splice(src, (11, 13), "120");
        assert_eq!(out, "local RI = 120\n");
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p carapace-preview provenance`
Expected: FAIL — `parse_params` / `splice` / types not found. (If Step 1's `full_moon` variant names differ from the characterization test, fix the test to the resolved API first, then proceed — that test is the source of truth for the shapes the impl uses.)

- [ ] **Step 4: Implement the scalar-param extraction + splice**

Prepend to `crates/carapace-preview/src/provenance.rs`:

```rust
//! Static provenance: parse `skin.lua` with full_moon to locate editable literals (top-level
//! `local` params and primitive call fields) by exact byte span, and splice edits back. Pairs with
//! the engine's per-node `Origin` (source line) to answer node-pick editability. See
//! docs/superpowers/specs/2026-07-01-carapace-preview-design.md.

use full_moon::ast::Stmt;
use full_moon::node::Node;

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub value: LiteralValue,
}

#[derive(Debug, Clone)]
pub enum LiteralValue {
    Scalar { text: String, span: (usize, usize) },
    Table { subfields: Vec<(String, ScalarSpan)> },
}

#[derive(Debug, Clone)]
pub struct ScalarSpan {
    pub text: String,
    pub span: (usize, usize),
}

/// Byte-splice `new_text` over `span` in `src`. The single write primitive — never regenerates.
pub fn splice(src: &str, span: (usize, usize), new_text: &str) -> String {
    let mut out = String::with_capacity(src.len());
    out.push_str(&src[..span.0]);
    out.push_str(new_text);
    out.push_str(&src[span.1..]);
    out
}

/// The exact source span of an expression, as byte offsets, if it has a range.
fn span_of(node: &impl Node) -> Option<(usize, usize)> {
    let (a, b) = node.range()?;
    Some((a.bytes(), b.bytes()))
}

/// Classify an expression as a scalar literal (number / string / boolean symbol) → its span+text.
/// Anything else (variable, binop, call, table, function) is not a scalar literal.
fn scalar_literal(src: &str, expr: &full_moon::ast::Expression) -> Option<LiteralValue> {
    use full_moon::ast::Expression;
    let is_scalar = match expr {
        Expression::Number(_) | Expression::String(_) => true,
        Expression::Symbol(sym) => {
            matches!(sym.token().to_string().as_str(), "true" | "false")
        }
        _ => false,
    };
    if !is_scalar {
        return None;
    }
    let span = span_of(expr)?;
    Some(LiteralValue::Scalar {
        text: src[span.0..span.1].to_string(),
        span,
    })
}

/// Top-level `local NAME = <scalar literal>` definitions (single name, single scalar value).
pub fn parse_params(src: &str) -> Vec<Param> {
    let Ok(ast) = full_moon::parse(src) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for stmt in ast.nodes().stmts() {
        let Stmt::LocalAssignment(la) = stmt else {
            continue;
        };
        let names: Vec<_> = la.names().iter().collect();
        let exprs: Vec<_> = la.expressions().iter().collect();
        if names.len() != 1 || exprs.len() != 1 {
            continue; // `local a, b = ...` unsupported (v1)
        }
        if let Some(value) = scalar_literal(src, exprs[0]) {
            out.push(Param {
                name: names[0].token().to_string(),
                value,
            });
        }
    }
    out
}
```

Add `mod provenance;` to `crates/carapace-preview/src/main.rs` (below the existing `mod` lines, ~line 8).

> Note: `TokenReference::token().to_string()` yields the identifier text (e.g. `RI`). If the resolved full_moon exposes the name differently, the characterization test in Step 2 will have shown the correct accessor — use it.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p carapace-preview provenance`
Expected: PASS (4 tests).

- [ ] **Step 6: Commit**

```bash
git add crates/carapace-preview/Cargo.toml crates/carapace-preview/src/provenance.rs crates/carapace-preview/src/main.rs
git commit -m "feat(preview): full_moon param extraction + span splice"
```

---

### Task 2: Primitive call-site indexing (literal fields + non-literal reasons)

**Files:**
- Modify: `crates/carapace-preview/src/provenance.rs`

**Interfaces:**
- Produces: `provenance::parse_source(src: &str) -> SourceModel` with `calls: Vec<CallSite>` populated for **top-level** primitive `Stmt::FunctionCall`s (chunk scope only — calls inside `for`/`if`/functions are not indexed here; those nodes are read-only via correlation). Each `CallSite { line, prim, fields }`; scalar-literal fields → `FieldState::Literal`, others → `FieldState::NonLiteral { reason }`.

- [ ] **Step 1: Write the failing tests**

Add to `provenance.rs` tests:

```rust
    #[test]
    fn indexes_top_level_call_fields_with_line_and_literacy() {
        let src = "fill{ x = 10, y = 20, color = STONE, tint = 1 + 2 }\n";
        let model = parse_source(src);
        assert_eq!(model.calls.len(), 1);
        let c = &model.calls[0];
        assert_eq!(c.prim, "fill");
        assert_eq!(c.line, 1);
        let f = |n: &str| c.fields.iter().find(|f| f.name == n).unwrap();
        match &f("x").state { FieldState::Literal { value: LiteralValue::Scalar { text, span } } => {
            assert_eq!(text, "10");
            assert_eq!(&src[span.0..span.1], "10");
        } _ => panic!("x literal") }
        assert!(matches!(f("color").state, FieldState::NonLiteral { .. }), "STONE is a variable");
        assert!(matches!(f("tint").state, FieldState::NonLiteral { .. }), "1+2 is an expression");
    }

    #[test]
    fn ignores_calls_nested_in_loops_at_this_stage() {
        let src = "for i=1,3 do\n  fill{ x = 10 }\nend\n";
        let model = parse_source(src);
        assert!(model.calls.is_empty(), "loop-nested calls are not top-level");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p carapace-preview provenance`
Expected: FAIL — `parse_source` / `SourceModel` / `CallSite` / `FieldInfo` / `FieldState` not found.

- [ ] **Step 3: Implement the types + call-site indexing**

Add to `provenance.rs` (types near the top, functions below `parse_params`):

```rust
#[derive(Debug, Clone, Default)]
pub struct SourceModel {
    pub params: Vec<Param>,
    pub calls: Vec<CallSite>,
}

#[derive(Debug, Clone)]
pub struct CallSite {
    pub line: u32,
    pub prim: String,
    pub fields: Vec<FieldInfo>,
}

#[derive(Debug, Clone)]
pub struct FieldInfo {
    pub name: String,
    pub state: FieldState,
}

#[derive(Debug, Clone)]
pub enum FieldState {
    Literal { value: LiteralValue },
    NonLiteral { reason: String },
}

/// Full model: params (Task 1) + top-level primitive call sites (this task).
pub fn parse_source(src: &str) -> SourceModel {
    SourceModel {
        params: parse_params(src),
        calls: parse_calls(src),
    }
}

/// The bare name a `FunctionCall` prefix targets, if it's a simple identifier (`fill`), plus the
/// table-constructor argument of a single `{...}` call. Returns `None` for anything else.
fn call_prim_and_table<'a>(
    fc: &'a full_moon::ast::FunctionCall,
) -> Option<(String, &'a full_moon::ast::TableConstructor)> {
    use full_moon::ast::{Call, FunctionArgs, Prefix, Suffix};
    let Prefix::Name(name) = fc.prefix() else {
        return None;
    };
    let prim = name.token().to_string();
    // Exactly one suffix: an anonymous call taking a table constructor: `fill{ ... }`.
    let mut suffixes = fc.suffixes();
    let (first, second) = (suffixes.next(), suffixes.next());
    if second.is_some() {
        return None;
    }
    match first {
        Some(Suffix::Call(Call::AnonymousCall(FunctionArgs::TableConstructor(t)))) => {
            Some((prim, t))
        }
        _ => None,
    }
}

fn parse_calls(src: &str) -> Vec<CallSite> {
    let Ok(ast) = full_moon::parse(src) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for stmt in ast.nodes().stmts() {
        let Stmt::FunctionCall(fc) = stmt else {
            continue;
        };
        let Some((prim, table)) = call_prim_and_table(fc) else {
            continue;
        };
        let line = fc.start_position().map(|p| p.line() as u32).unwrap_or(0);
        let fields = table_fields(src, table);
        out.push(CallSite { line, prim, fields });
    }
    out
}

/// Index the `NameKey` fields (`x = 10`, `color = {...}`) of a table constructor.
fn table_fields(src: &str, table: &full_moon::ast::TableConstructor) -> Vec<FieldInfo> {
    use full_moon::ast::Field;
    let mut out = Vec::new();
    for field in table.fields() {
        let Field::NameKey { key, value, .. } = field else {
            continue; // positional / [expr]-key fields: not addressable by name (v1)
        };
        let name = key.token().to_string();
        let state = match scalar_literal(src, value) {
            Some(value) => FieldState::Literal { value },
            None => FieldState::NonLiteral {
                reason: non_literal_reason(value),
            },
        };
        out.push(FieldInfo { name, state });
    }
    out
}

/// A short human reason a field isn't an editable scalar literal.
fn non_literal_reason(expr: &full_moon::ast::Expression) -> String {
    use full_moon::ast::Expression;
    match expr {
        Expression::Var(_) => "bound to a variable".to_string(),
        Expression::TableConstructor(_) => "table value".to_string(),
        Expression::FunctionCall(_) => "computed by a call".to_string(),
        Expression::BinaryOperator { .. } | Expression::UnaryOperator { .. } => {
            "computed expression".to_string()
        }
        _ => "not a literal".to_string(),
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p carapace-preview provenance`
Expected: PASS. (`parse_source(src).params` reuses Task 1; existing param tests still pass.)

- [ ] **Step 5: Commit**

```bash
git add crates/carapace-preview/src/provenance.rs
git commit -m "feat(preview): index top-level primitive call literal fields"
```

---

### Task 3: Table (color) literals — editable numeric sub-fields

**Files:**
- Modify: `crates/carapace-preview/src/provenance.rs`

**Interfaces:**
- Produces: `scalar_literal` extended so a `TableConstructor` of **all-numeric NameKey fields** (e.g. `{r=1,g=2,b=3}`) becomes `LiteralValue::Table { subfields }`, each subfield a `ScalarSpan`. Applies to both params (`local STONE = {r=..}`) and call fields (`color = {r=..}`). Non-numeric or nested tables stay `NonLiteral`/absent.

- [ ] **Step 1: Write the failing tests**

Add to `provenance.rs` tests:

```rust
    #[test]
    fn color_table_param_exposes_numeric_subfields() {
        let src = "local STONE = {r=10, g=20, b=30}\n";
        let params = parse_params(src);
        let p = &params[0];
        match &p.value {
            LiteralValue::Table { subfields } => {
                let g = subfields.iter().find(|(n, _)| n == "g").unwrap();
                assert_eq!(g.1.text, "20");
                assert_eq!(&src[g.1.span.0..g.1.span.1], "20");
            }
            _ => panic!("expected table"),
        }
    }

    #[test]
    fn color_field_on_a_call_is_editable_as_table() {
        let src = "fill{ color = {r=1, g=2, b=3} }\n";
        let c = &parse_source(src).calls[0];
        let color = c.fields.iter().find(|f| f.name == "color").unwrap();
        assert!(matches!(&color.state, FieldState::Literal { value: LiteralValue::Table { .. } }));
    }

    #[test]
    fn non_numeric_table_is_not_a_literal() {
        // A table with a nested table / non-numeric value is not an editable literal.
        let src = "fill{ meta = {name=\"x\"} }\n";
        let c = &parse_source(src).calls[0];
        let meta = c.fields.iter().find(|f| f.name == "meta").unwrap();
        assert!(matches!(meta.state, FieldState::NonLiteral { .. }));
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p carapace-preview provenance`
Expected: FAIL — table params currently skipped; color fields currently `NonLiteral`.

- [ ] **Step 3: Extend `scalar_literal` to a `literal_value` that also matches numeric tables**

In `provenance.rs`, rename the classifier to `literal_value` and add the table arm; update its two callers (`parse_params` via a new call, and `table_fields`). Replace `scalar_literal` with:

```rust
/// Classify an expression as an editable literal: a scalar (number/string/bool) or a table whose
/// every `NameKey` value is a number (colors, sizes). Anything else → `None`.
fn literal_value(src: &str, expr: &full_moon::ast::Expression) -> Option<LiteralValue> {
    use full_moon::ast::{Expression, Field};
    match expr {
        Expression::Number(_) | Expression::String(_) => {
            let span = span_of(expr)?;
            Some(LiteralValue::Scalar { text: src[span.0..span.1].to_string(), span })
        }
        Expression::Symbol(sym) if matches!(sym.token().to_string().as_str(), "true" | "false") => {
            let span = span_of(expr)?;
            Some(LiteralValue::Scalar { text: src[span.0..span.1].to_string(), span })
        }
        Expression::TableConstructor(t) => {
            let mut subfields = Vec::new();
            for field in t.fields() {
                let Field::NameKey { key, value, .. } = field else { return None };
                // Every subfield must be a bare number literal.
                match value {
                    Expression::Number(_) => {
                        let span = span_of(value)?;
                        subfields.push((
                            key.token().to_string(),
                            ScalarSpan { text: src[span.0..span.1].to_string(), span },
                        ));
                    }
                    _ => return None,
                }
            }
            if subfields.is_empty() { None } else { Some(LiteralValue::Table { subfields }) }
        }
        _ => None,
    }
}
```

Update `parse_params` to call `literal_value(src, exprs[0])` (instead of `scalar_literal`), and `table_fields` to call `literal_value(src, value)`. Delete the old `scalar_literal`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p carapace-preview provenance`
Expected: PASS (all provenance tests, old + new).

- [ ] **Step 5: Commit**

```bash
git add crates/carapace-preview/src/provenance.rs
git commit -m "feat(preview): editable numeric sub-fields for color/table literals"
```

---

### Task 4: Node-pick correlation → `NodeInfo` (`inspector.rs`)

**Files:**
- Create: `crates/carapace-preview/src/inspector.rs`
- Modify: `crates/carapace-preview/src/main.rs` (add `mod inspector;`)

**Interfaces:**
- Consumes: `provenance::{SourceModel, CallSite, FieldState, LiteralValue}`, `carapace::scene::Origin`.
- Produces:
  - `inspector::NodeInfo { pub prim: String, pub line: u32, pub props: Vec<PropInfo> }`, `PropInfo { pub name: String, pub editable: bool, pub value: Option<String>, pub reason: Option<String> }`.
  - `inspector::node_info(origins: &[Origin], picked: usize, model: &SourceModel) -> Option<NodeInfo>` — the pure correlation: picked node → its origin line + call → the `CallSite` at that line → editable/read-only per field, with **loop detection** (a line reached by more than one distinct `call` ordinal ⇒ all its props read-only "from a loop").

This is a pure function over plain data — no engine/GPU — so it is fully unit-tested.

- [ ] **Step 1: Write the failing tests**

Create `crates/carapace-preview/src/inspector.rs` with tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::provenance::{parse_source, SourceModel};
    use carapace::scene::Origin;

    fn model(src: &str) -> SourceModel { parse_source(src) }

    #[test]
    fn single_call_yields_editable_and_readonly_props() {
        let src = "fill{ x = 10, color = STONE }\n";
        let m = model(src);
        // One node from the fill on line 1, call 0.
        let origins = vec![Origin { line: Some(1), call: Some(0) }];
        let info = node_info(&origins, 0, &m).unwrap();
        assert_eq!(info.prim, "fill");
        let x = info.props.iter().find(|p| p.name == "x").unwrap();
        assert!(x.editable && x.value.as_deref() == Some("10"));
        let color = info.props.iter().find(|p| p.name == "color").unwrap();
        assert!(!color.editable && color.reason.is_some());
    }

    #[test]
    fn loop_generated_node_is_read_only() {
        let src = "for i=1,2 do\n  fill{ x = 10 }\nend\n";
        let m = model(src); // no top-level calls indexed (loop body)
        // Two nodes, same line 2, distinct calls 0 and 1 → loop.
        let origins = vec![
            Origin { line: Some(2), call: Some(0) },
            Origin { line: Some(2), call: Some(1) },
        ];
        let info = node_info(&origins, 0, &m).unwrap();
        assert!(info.props.iter().all(|p| !p.editable));
        assert!(info.props.iter().all(|p| p.reason.as_deref() == Some("from a loop")
            || p.reason.is_some()));
    }

    #[test]
    fn generated_node_call_none_has_no_info() {
        let m = model("list{ collection='c' }\n");
        let origins = vec![Origin { line: Some(1), call: None }];
        assert!(node_info(&origins, 0, &m).is_none(), "engine-generated → not inspectable");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p carapace-preview inspector`
Expected: FAIL — `node_info` / `NodeInfo` not found.

- [ ] **Step 3: Implement the correlation**

Prepend to `crates/carapace-preview/src/inspector.rs`:

```rust
//! Correlate a picked scene node (via its engine `Origin`) with the static `SourceModel` to decide
//! which of its props are editable literals vs read-only (bound / computed / loop-generated).

use crate::provenance::{FieldState, LiteralValue, SourceModel};
use carapace::scene::Origin;

#[derive(Debug, Clone)]
pub struct NodeInfo {
    pub prim: String,
    pub line: u32,
    pub props: Vec<PropInfo>,
}

#[derive(Debug, Clone)]
pub struct PropInfo {
    pub name: String,
    pub editable: bool,
    pub value: Option<String>,   // literal text for editable scalar props
    pub reason: Option<String>,  // why read-only
}

fn literal_display(v: &LiteralValue) -> String {
    match v {
        LiteralValue::Scalar { text, .. } => text.clone(),
        LiteralValue::Table { subfields } => {
            let inner: Vec<String> = subfields.iter().map(|(n, s)| format!("{n}={}", s.text)).collect();
            format!("{{{}}}", inner.join(", "))
        }
    }
}

/// Build inspector info for the picked node. `None` if the node is engine-generated (`call: None`),
/// has no source line, or no top-level call maps to that line (e.g. loop-nested and not indexed).
pub fn node_info(origins: &[Origin], picked: usize, model: &SourceModel) -> Option<NodeInfo> {
    let origin = origins.get(picked)?;
    origin.call?; // engine-generated rows/highlight are not inspectable
    let line = origin.line?;

    // Loop detection: does this source line back more than one distinct call ordinal?
    let distinct_calls = origins
        .iter()
        .filter(|o| o.line == Some(line))
        .filter_map(|o| o.call)
        .collect::<std::collections::BTreeSet<_>>();
    let looped = distinct_calls.len() > 1;

    let call = model.calls.iter().find(|c| c.line == line)?;
    let props = call
        .fields
        .iter()
        .map(|f| match (&f.state, looped) {
            (_, true) => PropInfo {
                name: f.name.clone(),
                editable: false,
                value: None,
                reason: Some("from a loop".to_string()),
            },
            (FieldState::Literal { value }, false) => PropInfo {
                name: f.name.clone(),
                editable: true,
                value: Some(literal_display(value)),
                reason: None,
            },
            (FieldState::NonLiteral { reason }, false) => PropInfo {
                name: f.name.clone(),
                editable: false,
                value: None,
                reason: Some(reason.clone()),
            },
        })
        .collect();

    Some(NodeInfo { prim: call.prim.clone(), line, props })
}
```

Add `mod inspector;` to `crates/carapace-preview/src/main.rs`.

> Note the second test: with the fill nested in a loop, `parse_source` indexes **no** top-level call at line 2, so `model.calls.iter().find(...)` returns `None` and `node_info` returns `None` — which the test should reflect. Adjust the `loop_generated_node_is_read_only` test to index the call: for a *reliably read-only-but-known* case, place the fill at top level too. Simplest correct fixture: keep the loop test asserting `node_info(...).is_none()` when the call is loop-nested (not indexed), and cover the "same line, multiple calls ⇒ looped flag" path with a top-level fixture where the call **is** indexed. Use:
> ```rust
> let src = "fill{ x = 10 }\n"; // indexed at line 1
> let origins = vec![Origin{line:Some(1),call:Some(0)}, Origin{line:Some(1),call:Some(1)}];
> let info = node_info(&origins, 0, &model(src)).unwrap();
> assert!(info.props.iter().all(|p| !p.editable && p.reason.as_deref()==Some("from a loop")));
> ```
> (Two calls reported on one indexed line ⇒ the loop flag trips even though the AST call is single — the origin ordinals are the authority. Update Step 1's loop test to this fixture before implementing.)

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p carapace-preview inspector`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/carapace-preview/src/inspector.rs crates/carapace-preview/src/main.rs
git commit -m "feat(preview): correlate picked node + origins → editable NodeInfo"
```

---

### Task 5: Protocol — Pick/SetProp/SetParam (up) + NodeInfo/Params (down)

**Files:**
- Modify: `crates/carapace-preview/src/protocol.rs`

**Interfaces:**
- Produces:
  - `ClientMsg` gains `Pick { x: f32, y: f32 }`, `SetProp { line: u32, field: String, value: serde_json::Value }`, `SetParam { name: String, field: Option<String>, value: serde_json::Value }` (`field` set for a color sub-field like `r`).
  - `OutMsg` gains `NodeInfo { json: serde_json::Value }` and `Params { json: serde_json::Value }` (pre-serialized payloads); `out_to_ws` maps both to `{"type":"nodeInfo",...}` / `{"type":"params",...}` text frames.

- [ ] **Step 1: Write the failing tests**

Add to `protocol.rs` tests:

```rust
    #[test]
    fn parses_pick_and_setprop_and_setparam() {
        let p = parse_client_msg(r#"{"type":"pick","x":5.0,"y":6.0}"#).unwrap();
        assert!(matches!(p, ClientMsg::Pick { x, y } if x == 5.0 && y == 6.0));
        let sp = parse_client_msg(r#"{"type":"setProp","line":3,"field":"x","value":12}"#).unwrap();
        assert!(matches!(sp, ClientMsg::SetProp { line: 3, ref field, .. } if field == "x"));
        let pr = parse_client_msg(r#"{"type":"setParam","name":"RI","field":null,"value":90}"#).unwrap();
        assert!(matches!(pr, ClientMsg::SetParam { ref name, field: None, .. } if name == "RI"));
        let prc = parse_client_msg(r#"{"type":"setParam","name":"STONE","field":"r","value":10}"#).unwrap();
        assert!(matches!(prc, ClientMsg::SetParam { field: Some(ref f), .. } if f == "r"));
    }

    #[test]
    fn nodeinfo_and_params_map_to_typed_text() {
        let ni = out_to_ws(&OutMsg::NodeInfo { json: serde_json::json!({"prim":"fill"}) });
        assert!(ni.is_text() && ni.into_text().unwrap().contains("\"type\":\"nodeInfo\""));
        let ps = out_to_ws(&OutMsg::Params { json: serde_json::json!([]) });
        assert!(ps.is_text() && ps.into_text().unwrap().contains("\"type\":\"params\""));
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p carapace-preview protocol`
Expected: FAIL — new variants not found.

- [ ] **Step 3: Implement the protocol additions**

In `protocol.rs`, add to the `ClientMsg` enum (after `SetCanvas`):
```rust
    Pick {
        x: f32,
        y: f32,
    },
    SetProp {
        line: u32,
        field: String,
        value: serde_json::Value,
    },
    SetParam {
        name: String,
        #[serde(default)]
        field: Option<String>,
        value: serde_json::Value,
    },
```

Add to the `OutMsg` enum (after `Error`):
```rust
    NodeInfo { json: serde_json::Value },
    Params { json: serde_json::Value },
```

Add to `out_to_ws`'s match (before the closing brace):
```rust
        OutMsg::NodeInfo { json } => {
            tungstenite::Message::text(json!({"type":"nodeInfo","info":json}).to_string())
        }
        OutMsg::Params { json } => {
            tungstenite::Message::text(json!({"type":"params","params":json}).to_string())
        }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p carapace-preview protocol`
Expected: PASS (old + new).

- [ ] **Step 5: Commit**

```bash
git add crates/carapace-preview/src/protocol.rs
git commit -m "feat(preview): protocol for pick/setProp/setParam + nodeInfo/params"
```

---

### Task 6: `SkinSession` — source model, node pick, write-back to disk

**Files:**
- Modify: `crates/carapace-preview/src/skin_session.rs`

**Interfaces:**
- Consumes: `provenance`, `inspector`, engine `layout_with_origins`/`Scene::pick`, `carapace::scene::Pt`.
- Produces (methods on `SkinSession`):
  - `pub fn entry_path(&self) -> PathBuf` — the on-disk Lua file (`dir.join(entry)`).
  - `pub fn source_model(&self) -> provenance::SourceModel` — parse the current on-disk Lua.
  - `pub fn params_json(&self) -> serde_json::Value` — the params list for the browser panel.
  - `pub fn pick(&self, w: f32, h: f32, p: Pt) -> Option<inspector::NodeInfo>` — layout_with_origins → Scene::pick → node_info.
  - `pub fn apply_prop(&self, line: u32, field: &str, value: &str) -> Result<(), String>` and `pub fn apply_param(&self, name: &str, sub: Option<&str>, value: &str) -> Result<(), String>` — **re-parse the current file**, find the target literal span, splice, write the file back. (The watcher then reloads.)

- [ ] **Step 1: Write the failing tests**

Add to `skin_session.rs` tests (the `tmp_skin` helper already exists):

```rust
    #[test]
    fn source_model_reads_params_from_disk() {
        let dir = tmp_skin("local RI = 90\nfill{ path = {{x=0,y=0},{x=1,y=0},{x=1,y=1}}, color = {r=1,g=2,b=3} }");
        let s = SkinSession::new(dir, Default::default(), Default::default());
        let m = s.source_model();
        assert!(m.params.iter().any(|p| p.name == "RI"));
    }

    #[test]
    fn apply_param_rewrites_the_file() {
        let dir = tmp_skin("local RI = 90\nfill{ path = {{x=0,y=0},{x=1,y=0},{x=1,y=1}}, color = {r=1,g=2,b=3} }");
        let s = SkinSession::new(dir.clone(), Default::default(), Default::default());
        s.apply_param("RI", None, "120").unwrap();
        let on_disk = std::fs::read_to_string(s.entry_path()).unwrap();
        assert!(on_disk.starts_with("local RI = 120"), "got: {on_disk}");
    }

    #[test]
    fn apply_prop_rewrites_a_call_field() {
        let dir = tmp_skin("fill{ x = 10, path = {{x=0,y=0},{x=1,y=0},{x=1,y=1}}, color = {r=1,g=2,b=3} }");
        let s = SkinSession::new(dir.clone(), Default::default(), Default::default());
        s.apply_prop(1, "x", "42").unwrap();
        let on_disk = std::fs::read_to_string(s.entry_path()).unwrap();
        assert!(on_disk.contains("x = 42"), "got: {on_disk}");
    }

    #[test]
    fn apply_param_color_subfield() {
        let dir = tmp_skin("local STONE = {r=10, g=20, b=30}\nfill{ path = {{x=0,y=0},{x=1,y=0},{x=1,y=1}}, color = STONE }");
        let s = SkinSession::new(dir.clone(), Default::default(), Default::default());
        s.apply_param("STONE", Some("g"), "99").unwrap();
        let on_disk = std::fs::read_to_string(s.entry_path()).unwrap();
        assert!(on_disk.contains("g=99"), "got: {on_disk}");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p carapace-preview skin_session`
Expected: FAIL — new methods not found.

- [ ] **Step 3: Implement the methods**

In `skin_session.rs`: store the entry filename and add the methods. Changes:

1. Add a field `entry: String` to `SkinSession` (after `pub canvas`), and set it in `build_engine`/`reload`. Update `build_engine` to also return the entry:
   ```rust
   fn build_engine(
       dir: &Path,
       values: &Values,
       log: &ActionLog,
   ) -> Result<(Engine, String, (u32, u32), String), String> {
       let (manifest, source) = carapace::skin::load_dir(dir).map_err(|e| format!("{e:?}"))?;
       let actions: Vec<ActionSpec> = scan_actions(&source.lua_src)
           .into_iter()
           .map(|name| ActionSpec { name })
           .collect();
       let host: Box<dyn Host> = Box::new(PreviewHost::new(values.clone(), log.clone(), actions));
       let engine = Engine::new(host, VocabRegistry::base(), source).map_err(|e| format!("{e:?}"))?;
       Ok((engine, manifest.name, (manifest.canvas.width, manifest.canvas.height), manifest.entry))
   }
   ```
   In `SkinSession::new`, initialize `entry: "skin.lua".to_string()`. In `reload`, bind the 4-tuple and set `self.entry = entry;`.

2. Add methods to `impl SkinSession`:
   ```rust
   pub fn entry_path(&self) -> PathBuf {
       self.dir.join(&self.entry)
   }

   fn read_source(&self) -> String {
       std::fs::read_to_string(self.entry_path()).unwrap_or_default()
   }

   pub fn source_model(&self) -> crate::provenance::SourceModel {
       crate::provenance::parse_source(&self.read_source())
   }

   pub fn params_json(&self) -> serde_json::Value {
       use crate::provenance::LiteralValue;
       let m = self.source_model();
       let items: Vec<serde_json::Value> = m
           .params
           .iter()
           .map(|p| match &p.value {
               LiteralValue::Scalar { text, .. } => {
                   serde_json::json!({"name": p.name, "kind": "scalar", "value": text})
               }
               LiteralValue::Table { subfields } => {
                   let subs: Vec<_> = subfields
                       .iter()
                       .map(|(n, s)| serde_json::json!({"name": n, "value": s.text}))
                       .collect();
                   serde_json::json!({"name": p.name, "kind": "table", "subfields": subs})
               }
           })
           .collect();
       serde_json::Value::Array(items)
   }

   pub fn pick(
       &self,
       w: f32,
       h: f32,
       p: carapace::scene::Pt,
   ) -> Option<crate::inspector::NodeInfo> {
       let engine = self.engine.as_ref()?;
       let (scene, origins) = engine.layout_with_origins(w, h);
       let idx = scene.pick(p)?;
       crate::inspector::node_info(&origins, idx, &self.source_model())
   }

   pub fn apply_prop(&self, line: u32, field: &str, value: &str) -> Result<(), String> {
       use crate::provenance::{FieldState, LiteralValue};
       let src = self.read_source();
       let model = crate::provenance::parse_source(&src);
       let call = model
           .calls
           .iter()
           .find(|c| c.line == line)
           .ok_or_else(|| format!("no call at line {line}"))?;
       let f = call
           .fields
           .iter()
           .find(|f| f.name == field)
           .ok_or_else(|| format!("no field {field}"))?;
       let FieldState::Literal { value: LiteralValue::Scalar { span, .. } } = &f.state else {
           return Err(format!("{field} is not an editable scalar"));
       };
       let out = crate::provenance::splice(&src, *span, value);
       std::fs::write(self.entry_path(), out).map_err(|e| e.to_string())
   }

   pub fn apply_param(&self, name: &str, sub: Option<&str>, value: &str) -> Result<(), String> {
       use crate::provenance::LiteralValue;
       let src = self.read_source();
       let model = crate::provenance::parse_source(&src);
       let param = model
           .params
           .iter()
           .find(|p| p.name == name)
           .ok_or_else(|| format!("no param {name}"))?;
       let span = match (&param.value, sub) {
           (LiteralValue::Scalar { span, .. }, None) => *span,
           (LiteralValue::Table { subfields }, Some(sub)) => {
               subfields
                   .iter()
                   .find(|(n, _)| n == sub)
                   .map(|(_, s)| s.span)
                   .ok_or_else(|| format!("no subfield {sub}"))?
           }
           _ => return Err("param/subfield mismatch".to_string()),
       };
       let out = crate::provenance::splice(&src, span, value);
       std::fs::write(self.entry_path(), out).map_err(|e| e.to_string())
   }
   ```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p carapace-preview skin_session`
Expected: PASS (old + 4 new). The `pick` method needs a GPU only if called; these tests don't call it (pick is covered by `inspector` unit tests + manual verification).

- [ ] **Step 5: Commit**

```bash
git add crates/carapace-preview/src/skin_session.rs
git commit -m "feat(preview): source model, node pick, and write-back to disk"
```

---

### Task 7: Wire `main.rs` — handle Pick/SetProp/SetParam; broadcast Params

**Files:**
- Modify: `crates/carapace-preview/src/main.rs`

**Interfaces:**
- Consumes: the new `ClientMsg`/`OutMsg` variants and `SkinSession` methods.
- Produces: engine-loop handling so a `pick` returns `NodeInfo`, `setProp`/`setParam` rewrite the file (watcher reloads), and each client gets `Params` on connect and after every reload.

There are no new unit tests here (integration wiring over channels); it is validated by the build + the manual verification in Task 8. Keep edits minimal and mirror existing arms.

- [ ] **Step 1: Send `Params` on client connect**

In `run_engine_loop`'s `EngineMsg::ClientConnected(tx)` arm (`main.rs:120-134`), after sending `Error` and before `clients.push(tx)`, add:
```rust
                    let _ = tx.send(OutMsg::Params { json: session.params_json() });
```

- [ ] **Step 2: Handle `Pick`**

Add a new match arm alongside the other `EngineMsg::Client(...)` arms:
```rust
                EngineMsg::Client(ClientMsg::Pick { x, y }) => {
                    if let Some(info) = session.pick(render_size.0 as f32, render_size.1 as f32, Pt { x, y }) {
                        let json = serde_json::json!({
                            "prim": info.prim,
                            "line": info.line,
                            "props": info.props.iter().map(|p| serde_json::json!({
                                "name": p.name, "editable": p.editable,
                                "value": p.value, "reason": p.reason,
                            })).collect::<Vec<_>>(),
                        });
                        broadcast(&mut clients, &OutMsg::NodeInfo { json });
                    }
                }
```

- [ ] **Step 3: Handle `SetProp` / `SetParam` (write-back; watcher reloads)**

Add arms:
```rust
                EngineMsg::Client(ClientMsg::SetProp { line, field, value }) => {
                    let text = json_scalar_to_lua(&value);
                    if let Err(e) = session.apply_prop(line, &field, &text) {
                        eprintln!("setProp failed: {e}");
                    }
                    // The file watcher fires a Reload; no explicit re-render here.
                }
                EngineMsg::Client(ClientMsg::SetParam { name, field, value }) => {
                    let text = json_scalar_to_lua(&value);
                    if let Err(e) = session.apply_param(&name, field.as_deref(), &text) {
                        eprintln!("setParam failed: {e}");
                    }
                }
```

- [ ] **Step 4: Broadcast `Params` after each reload**

In the `EngineMsg::Reload` arm (`main.rs:171-195`), after the `Meta` broadcast, add:
```rust
                    broadcast(&mut clients, &OutMsg::Params { json: session.params_json() });
```

- [ ] **Step 5: Add the JSON→Lua literal helper**

Add near `json_to_state` (`main.rs:234`):
```rust
/// Render a JSON scalar as the Lua literal text to splice into the source. Numbers pass through;
/// strings become quoted Lua strings; booleans become `true`/`false`.
fn json_scalar_to_lua(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::String(s) => format!("{s:?}"), // Rust debug = valid Lua double-quoted string
        _ => "nil".to_string(),
    }
}
```

- [ ] **Step 6: Build + full test + clippy**

Run:
```bash
cargo build -p carapace-preview
cargo test -p carapace-preview
cargo clippy -p carapace-preview --all-targets -- -D warnings
```
Expected: clean build, all tests pass, no clippy warnings.

- [ ] **Step 7: Commit**

```bash
git add crates/carapace-preview/src/main.rs
git commit -m "feat(preview): wire pick + write-back into the engine loop"
```

---

### Task 8: Browser UI — inspector + parameters panels + Inspect toggle

**Files:**
- Modify: `crates/carapace-preview/assets/index.html`

**Interfaces:**
- Consumes: down `nodeInfo` / `params`; up `pick` / `setProp` / `setParam`. Reuses the existing WS `send()` + `ws.onmessage` dispatch.

The single-file viewer JS is thin and validated by eye against a running server (repo precedent). One small unit test guards the templating; the rest is manual.

- [ ] **Step 1: Add panels + an Inspect toggle to the HTML**

In `assets/index.html`, inside `#panel`, add before `<h2>Action log</h2>`:
```html
  <h2>Parameters</h2>
  <div id="params"></div>

  <h2>Inspector <label style="font-weight:400"><input type="checkbox" id="inspectToggle" /> click to inspect</label></h2>
  <div id="inspector"><div id="status2">click a node…</div></div>
```

- [ ] **Step 2: Handle `params` and `nodeInfo` in `ws.onmessage`**

In the `ws.onmessage` JSON branch (after the existing `else if (msg.type === "error")` block), add:
```js
    else if (msg.type === "params") { renderParams(msg.params); }
    else if (msg.type === "nodeInfo") { renderInspector(msg.info); }
```

- [ ] **Step 3: Route canvas clicks to `pick` when Inspect is on**

Replace the existing `canvas.addEventListener("click", …)` body so it sends `pick` when the toggle is checked, else the existing `pointer`:
```js
  canvas.addEventListener("click", (e) => {
    const r = canvas.getBoundingClientRect();
    const x = (e.clientX - r.left) * (designW / r.width);
    const y = (e.clientY - r.top) * (designH / r.height);
    send({ type: document.getElementById("inspectToggle").checked ? "pick" : "pointer", x, y });
  });
```

- [ ] **Step 4: Add the render + edit functions**

Before the closing `</script>`, add:
```js
  function renderParams(params) {
    const el = document.getElementById("params");
    el.innerHTML = "";
    for (const p of params) {
      if (p.kind === "scalar") {
        el.appendChild(paramRow(p.name, p.value, () =>
          send({ type: "setParam", name: p.name, field: null, value: coerce(inp.value) })));
      } else if (p.kind === "table") {
        const label = document.createElement("div"); label.textContent = p.name; label.style.color = "#999";
        el.appendChild(label);
        for (const s of p.subfields) {
          el.appendChild(paramRow("· " + s.name, s.value, (v) =>
            send({ type: "setParam", name: p.name, field: s.name, value: coerce(v) })));
        }
      }
    }
  }

  // Generic labelled input row that fires `onCommit(value)` on change.
  function paramRow(name, value, onCommit) {
    const row = document.createElement("div"); row.className = "row";
    const k = document.createElement("span"); k.textContent = name; k.style.flex = "1";
    const v = document.createElement("input"); v.value = value; v.style.flex = "1";
    v.addEventListener("change", () => onCommit(v.value));
    row.appendChild(k); row.appendChild(v);
    return row;
  }

  function renderInspector(info) {
    const el = document.getElementById("inspector");
    el.innerHTML = "";
    const title = document.createElement("div");
    title.textContent = info.prim + "  (line " + info.line + ")";
    title.style.fontWeight = "600";
    el.appendChild(title);
    for (const p of info.props) {
      if (p.editable) {
        const row = document.createElement("div"); row.className = "row";
        const k = document.createElement("span"); k.textContent = p.name; k.style.flex = "1";
        const v = document.createElement("input"); v.value = p.value ?? ""; v.style.flex = "1";
        v.addEventListener("change", () =>
          send({ type: "setProp", line: info.line, field: p.name, value: coerce(v.value) }));
        row.appendChild(k); row.appendChild(v); el.appendChild(row);
      } else {
        const row = document.createElement("div"); row.style.color = "#888"; row.style.fontSize = "12px";
        row.textContent = p.name + " — " + (p.reason || "read-only");
        el.appendChild(row);
      }
    }
  }

  // "12" → 12 (number); "true"/"false" → boolean; else the raw string.
  function coerce(s) {
    const t = s.trim();
    if (t === "true") return true;
    if (t === "false") return false;
    const n = parseFloat(t);
    return (t !== "" && !isNaN(n) && String(n) === t) ? n : s;
  }
```

> Note: `renderParams` references `inp` in the scalar branch — fix that to capture the input element: change the scalar branch to build the row via `paramRow(p.name, p.value, (v) => send({ type: "setParam", name: p.name, field: null, value: coerce(v) }))` so the committed value comes from the row's own input (`paramRow` passes `v.value` to `onCommit`). Ensure `paramRow`'s `onCommit` is called with the input's string value.

- [ ] **Step 5: Verify the templating test still passes + manual check**

Run: `cargo test -p carapace-preview server` — expected PASS (`render_index` still substitutes `{{WS_PORT}}`).

Manual: run `cargo run -p carapace-preview -- crates/carapace-demo/skins/<a-skin-with-params>` (pick any skin dir with a top-level `local` and a straight-line `fill{}`), open the browser:
- Parameters panel lists the `local` scalars/colors; editing one rewrites `skin.lua` and the preview updates within a moment.
- Toggle **Inspect**, click a straight-line fill → inspector shows its editable props; edit `x`/color → file updates, preview reflows.
- Click a looped/computed node → props show read-only with a reason.

- [ ] **Step 6: Commit**

```bash
git add crates/carapace-preview/assets/index.html
git commit -m "feat(preview): inspector + parameters panels with write-back UI"
```

---

### Task 9: README + workspace check

**Files:**
- Modify: `crates/carapace-preview/README.md`

- [ ] **Step 1: Document the editing surfaces**

Add a section to the README covering: the **Parameters** panel (edit top-level `local` scalars/colors), the **Inspector** (Inspect toggle → click a node → edit literal props), that edits **rewrite `skin.lua` on disk** (watcher reloads), and the **literal-only / one-primitive-per-line / no-text-pick / loop-is-read-only** limitations. Keep it concise (repo convention: full guides live in the future centralized Carapace API docs).

- [ ] **Step 2: Full workspace verification**

Run:
```bash
cargo test -p carapace-preview
cargo test -p carapace           # prerequisite engine change still green
cargo clippy -p carapace-preview --all-targets -- -D warnings
cargo fmt --check
```
Expected: all pass. If `fmt --check` flags files, run `cargo fmt` and amend.

- [ ] **Step 3: Commit**

```bash
git add crates/carapace-preview/README.md
git commit -m "docs(preview): README for inspector + parameters editing"
```

---

## Self-Review checklist (run after implementing)

- **Spec coverage:** parameters panel ✔ (Tasks 1,3,6,8); property inspector ✔ (Tasks 2,4,6,7,8); write-back to disk ✔ (Task 6); read-only + reason ✔ (Task 4); loop-generated read-only ✔ (Task 4); color/param editing (STONE_* recolor) ✔ (Task 3). Deferred & documented: text-node picking, multi-primitive-per-line, loop-bound-count params, from-scratch skin studio.
- **Prerequisite:** every use of `layout_with_origins`/`Origin`/`Scene::pick` matches the API delivered by `2026-07-03-scene-node-provenance.md`. Do not start this plan until that one is merged.
- **Write-back safety:** every edit re-parses the current file and splices one span; `json_scalar_to_lua` produces valid Lua; no file regeneration.
- **Type consistency:** `LiteralValue::{Scalar,Table}`, `FieldState::{Literal,NonLiteral}`, `NodeInfo/PropInfo`, and the `ClientMsg`/`OutMsg` variants are used with identical shapes across `provenance.rs`, `inspector.rs`, `skin_session.rs`, `protocol.rs`, and `main.rs`.
- **No placeholders:** every step has exact code, file anchors, and commands. The two full_moon variant-name caveats (Task 1 Step 4, Task 4 Step 3) are pinned by characterization tests, matching the core plan's tungstenite precedent.

## Execution note

Order is fixed: land `2026-07-03-scene-node-provenance.md` first (engine API), then this plan. Within this plan, Tasks 1→3 (provenance) and 5 (protocol) are independent of the engine change and could even start earlier; Tasks 4, 6, 7 depend on the engine API; Task 8 depends on 5–7.
</content>
