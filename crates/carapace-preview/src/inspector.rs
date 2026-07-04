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
pub struct SubFieldInfo {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone)]
pub struct PropInfo {
    pub name: String,
    pub editable: bool,
    pub value: Option<String>,  // literal text for editable scalar props
    pub reason: Option<String>, // why read-only
    pub subfields: Option<Vec<SubFieldInfo>>, // Some for an editable table/color field
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
                subfields: None,
            },
            (
                FieldState::Literal {
                    value: LiteralValue::Scalar { text, .. },
                },
                false,
            ) => PropInfo {
                name: f.name.clone(),
                editable: true,
                value: Some(text.clone()),
                reason: None,
                subfields: None,
            },
            (
                FieldState::Literal {
                    value: LiteralValue::Table { subfields },
                },
                false,
            ) => PropInfo {
                name: f.name.clone(),
                editable: true,
                value: None,
                reason: None,
                subfields: Some(
                    subfields
                        .iter()
                        .map(|(n, s)| SubFieldInfo {
                            name: n.clone(),
                            value: s.text.clone(),
                        })
                        .collect(),
                ),
            },
            (FieldState::NonLiteral { reason }, false) => PropInfo {
                name: f.name.clone(),
                editable: false,
                value: None,
                reason: Some(reason.clone()),
                subfields: None,
            },
        })
        .collect();

    Some(NodeInfo {
        prim: call.prim.clone(),
        line,
        props,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provenance::{SourceModel, parse_source};
    use carapace::scene::Origin;

    fn model(src: &str) -> SourceModel {
        parse_source(src)
    }

    #[test]
    fn single_call_yields_editable_and_readonly_props() {
        let src = "fill{ x = 10, color = STONE }\n";
        let m = model(src);
        // One node from the fill on line 1, call 0.
        let origins = vec![Origin {
            line: Some(1),
            call: Some(0),
        }];
        let info = node_info(&origins, 0, &m).unwrap();
        assert_eq!(info.prim, "fill");
        let x = info.props.iter().find(|p| p.name == "x").unwrap();
        assert!(x.editable && x.value.as_deref() == Some("10"));
        assert!(x.subfields.is_none());
        let color = info.props.iter().find(|p| p.name == "color").unwrap();
        assert!(!color.editable && color.reason.is_some());
        assert!(color.subfields.is_none());
    }

    #[test]
    fn inline_color_table_field_yields_editable_subfields() {
        let src = "fill{ color = {r=1, g=2, b=3} }\n";
        let m = model(src);
        let origins = vec![Origin {
            line: Some(1),
            call: Some(0),
        }];
        let info = node_info(&origins, 0, &m).unwrap();
        let color = info.props.iter().find(|p| p.name == "color").unwrap();
        assert!(color.editable);
        assert!(color.value.is_none());
        assert!(color.reason.is_none());
        let subs = color.subfields.as_ref().expect("subfields present");
        let names: Vec<&str> = subs.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["r", "g", "b"]);
        let values: Vec<&str> = subs.iter().map(|s| s.value.as_str()).collect();
        assert_eq!(values, vec!["1", "2", "3"]);
    }

    #[test]
    fn loop_generated_node_is_read_only() {
        // Corrected fixture (see brief Step 3 Note): a single indexed top-level call whose
        // line the origins report under two distinct `call` ordinals. Origin ordinals are
        // the authority for loop detection, not the AST — a loop-nested `fill` wouldn't even
        // be indexed as a top-level CallSite, so this is the fixture that actually exercises
        // the loop-detection path.
        let src = "fill{ x = 10 }\n"; // indexed at line 1
        let origins = vec![
            Origin {
                line: Some(1),
                call: Some(0),
            },
            Origin {
                line: Some(1),
                call: Some(1),
            },
        ];
        let info = node_info(&origins, 0, &model(src)).unwrap();
        assert!(
            info.props
                .iter()
                .all(|p| !p.editable && p.reason.as_deref() == Some("from a loop"))
        );
    }

    #[test]
    fn generated_node_call_none_has_no_info() {
        let m = model("list{ collection='c' }\n");
        let origins = vec![Origin {
            line: Some(1),
            call: None,
        }];
        assert!(
            node_info(&origins, 0, &m).is_none(),
            "engine-generated → not inspectable"
        );
    }
}
