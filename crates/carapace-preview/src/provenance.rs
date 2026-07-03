//! Static provenance: parse `skin.lua` with full_moon to locate editable literals (top-level
//! `local` params and primitive call fields) by exact byte span, and splice edits back. Pairs with
//! the engine's per-node `Origin` (source line) to answer node-pick editability. See
//! docs/superpowers/specs/2026-07-01-carapace-preview-design.md.
//!
use full_moon::ast::Stmt;
use full_moon::node::Node;

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub value: LiteralValue,
}

#[derive(Debug, Clone)]
pub enum LiteralValue {
    Scalar {
        text: String,
        span: (usize, usize),
    },
    Table {
        subfields: Vec<(String, ScalarSpan)>,
    },
}

#[derive(Debug, Clone)]
pub struct ScalarSpan {
    pub text: String,
    pub span: (usize, usize),
}

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

/// Classify an expression as an editable literal: a scalar (number/string/bool) or a table whose
/// every `NameKey` value is a number (colors, sizes). Anything else → `None`.
fn literal_value(src: &str, expr: &full_moon::ast::Expression) -> Option<LiteralValue> {
    use full_moon::ast::{Expression, Field};
    match expr {
        Expression::Number(_) | Expression::String(_) => {
            let span = span_of(expr)?;
            Some(LiteralValue::Scalar {
                text: src[span.0..span.1].to_string(),
                span,
            })
        }
        Expression::Symbol(sym) if matches!(sym.token().to_string().as_str(), "true" | "false") => {
            let span = span_of(expr)?;
            Some(LiteralValue::Scalar {
                text: src[span.0..span.1].to_string(),
                span,
            })
        }
        Expression::TableConstructor(t) => {
            let mut subfields = Vec::new();
            for field in t.fields() {
                let Field::NameKey { key, value, .. } = field else {
                    return None;
                };
                // Every subfield must be a bare number literal.
                match value {
                    Expression::Number(_) => {
                        let span = span_of(value)?;
                        subfields.push((
                            key.token().to_string(),
                            ScalarSpan {
                                text: src[span.0..span.1].to_string(),
                                span,
                            },
                        ));
                    }
                    _ => return None,
                }
            }
            if subfields.is_empty() {
                None
            } else {
                Some(LiteralValue::Table { subfields })
            }
        }
        _ => None,
    }
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
        if let Some(value) = literal_value(src, exprs[0]) {
            out.push(Param {
                name: names[0].token().to_string(),
                value,
            });
        }
    }
    out
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
fn call_prim_and_table(
    fc: &full_moon::ast::FunctionCall,
) -> Option<(String, &full_moon::ast::TableConstructor)> {
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
        let state = match literal_value(src, value) {
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
        let full_moon::ast::Stmt::LocalAssignment(la) = stmt else {
            panic!("expected local")
        };
        let expr = la.expressions().iter().next().expect("one expr");
        let (a, b) = expr.range().expect("range");
        assert_eq!(
            &src[a.bytes()..b.bytes()],
            "90",
            "range slices the literal exactly"
        );
    }

    #[test]
    fn extracts_scalar_params_with_spans() {
        let src = "local RI = 90\nlocal NAME = \"door\"\nlocal ON = true\n";
        let params = parse_params(src);
        let by = |n: &str| params.iter().find(|p| p.name == n).cloned().unwrap();
        match by("RI").value {
            LiteralValue::Scalar { text, span } => {
                assert_eq!(text, "90");
                assert_eq!(&src[span.0..span.1], "90");
            }
            _ => panic!("scalar"),
        }
        match by("NAME").value {
            LiteralValue::Scalar { text, .. } => assert_eq!(text, "\"door\""),
            _ => panic!(),
        }
        match by("ON").value {
            LiteralValue::Scalar { text, .. } => assert_eq!(text, "true"),
            _ => panic!(),
        }
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

    #[test]
    fn indexes_top_level_call_fields_with_line_and_literacy() {
        let src = "fill{ x = 10, y = 20, color = STONE, tint = 1 + 2 }\n";
        let model = parse_source(src);
        assert_eq!(model.calls.len(), 1);
        let c = &model.calls[0];
        assert_eq!(c.prim, "fill");
        assert_eq!(c.line, 1);
        let f = |n: &str| c.fields.iter().find(|f| f.name == n).unwrap();
        match &f("x").state {
            FieldState::Literal {
                value: LiteralValue::Scalar { text, span },
            } => {
                assert_eq!(text, "10");
                assert_eq!(&src[span.0..span.1], "10");
            }
            _ => panic!("x literal"),
        }
        assert!(
            matches!(f("color").state, FieldState::NonLiteral { .. }),
            "STONE is a variable"
        );
        assert!(
            matches!(f("tint").state, FieldState::NonLiteral { .. }),
            "1+2 is an expression"
        );
    }

    #[test]
    fn ignores_calls_nested_in_loops_at_this_stage() {
        let src = "for i=1,3 do\n  fill{ x = 10 }\nend\n";
        let model = parse_source(src);
        assert!(
            model.calls.is_empty(),
            "loop-nested calls are not top-level"
        );
    }

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
        assert!(matches!(
            &color.state,
            FieldState::Literal {
                value: LiteralValue::Table { .. }
            }
        ));
    }

    #[test]
    fn non_numeric_table_is_not_a_literal() {
        // A table with a nested table / non-numeric value is not an editable literal.
        let src = "fill{ meta = {name=\"x\"} }\n";
        let c = &parse_source(src).calls[0];
        let meta = c.fields.iter().find(|f| f.name == "meta").unwrap();
        assert!(matches!(meta.state, FieldState::NonLiteral { .. }));
    }
}
