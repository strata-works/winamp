//! Static provenance: parse `skin.lua` with full_moon to locate editable literals (top-level
//! `local` params and primitive call fields) by exact byte span, and splice edits back. Pairs with
//! the engine's per-node `Origin` (source line) to answer node-pick editability. See
//! docs/superpowers/specs/2026-07-01-carapace-preview-design.md.
//!
//! Not wired into `main.rs` yet — this task only adds the static extraction + splice primitives.
//! Later tasks (call-site indexing, correlation, protocol, UI) consume this module's public API.
#![allow(dead_code)]

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
}
