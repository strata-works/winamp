//! Wire protocol between the browser viewer and the engine thread over the WebSocket connection.
//! Consumed by the server task added in a later task — kept ungated so it's unit-tested now.

use serde::Deserialize;
use serde_json::json;

/// Messages the browser viewer sends up to the engine thread.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ClientMsg {
    Pointer {
        x: f32,
        y: f32,
    },
    SetValue {
        key: String,
        value: serde_json::Value,
    },
    SetCanvas {
        w: u32,
        h: u32,
    },
}

pub fn parse_client_msg(text: &str) -> Result<ClientMsg, serde_json::Error> {
    serde_json::from_str(text)
}

/// Messages the engine thread broadcasts down to each connected browser client.
#[derive(Debug, Clone)]
pub enum OutMsg {
    Frame(Vec<u8>), // PNG-encoded RGBA
    Meta { name: String, w: u32, h: u32 },
    ActionLog { action: String },
    Error { message: Option<String> },
}

pub fn out_to_ws(msg: &OutMsg) -> tungstenite::Message {
    match msg {
        OutMsg::Frame(bytes) => tungstenite::Message::binary(bytes.clone()),
        OutMsg::Meta { name, w, h } => {
            tungstenite::Message::text(json!({"type":"meta","name":name,"w":w,"h":h}).to_string())
        }
        OutMsg::ActionLog { action } => {
            tungstenite::Message::text(json!({"type":"actionLog","action":action}).to_string())
        }
        OutMsg::Error { message } => {
            tungstenite::Message::text(json!({"type":"error","message":message}).to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pointer() {
        let m = parse_client_msg(r#"{"type":"pointer","x":12.5,"y":7.0}"#).unwrap();
        assert!(matches!(m, ClientMsg::Pointer { x, y } if x == 12.5 && y == 7.0));
    }

    #[test]
    fn parses_set_value_number_and_string() {
        let n = parse_client_msg(r#"{"type":"setValue","key":"level","value":0.4}"#).unwrap();
        assert!(
            matches!(n, ClientMsg::SetValue { ref key, value: serde_json::Value::Number(_) } if key == "level")
        );
        let s = parse_client_msg(r#"{"type":"setValue","key":"track","value":"Song"}"#).unwrap();
        assert!(matches!(
            s,
            ClientMsg::SetValue {
                value: serde_json::Value::String(_),
                ..
            }
        ));
    }

    #[test]
    fn parses_set_canvas() {
        let m = parse_client_msg(r#"{"type":"setCanvas","w":320,"h":200}"#).unwrap();
        assert!(matches!(m, ClientMsg::SetCanvas { w: 320, h: 200 }));
    }

    #[test]
    fn frame_maps_to_binary_others_to_text() {
        let f = out_to_ws(&OutMsg::Frame(vec![1, 2, 3]));
        assert!(f.is_binary());
        let meta = out_to_ws(&OutMsg::Meta {
            name: "S".into(),
            w: 300,
            h: 120,
        });
        assert!(meta.is_text());
        assert!(meta.into_text().unwrap().contains("\"type\":\"meta\""));
    }
}
