//! carapace-preview — a live, interactive browser previewer for carapace skins.
//! See docs/superpowers/specs/2026-07-01-carapace-preview-design.md.

mod inspector;
mod preview_host;
mod protocol;
mod provenance;
mod render;
mod server;
mod skin_session;

use carapace::engine::PointerEvent;
use carapace::scene::Pt;
use carapace::state::StateValue;
use preview_host::{ActionLog, Values};
use protocol::{ClientMsg, OutMsg};
use server::{EngineMsg, serve};
use skin_session::SkinSession;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(skin_dir) = args.next() else {
        eprintln!("usage: carapace-preview <skin-dir> [--port <n>]");
        return ExitCode::FAILURE;
    };
    let mut port: u16 = 0; // 0 = ephemeral
    while let Some(a) = args.next() {
        if a == "--port" {
            port = args.next().and_then(|p| p.parse().ok()).unwrap_or(0);
        }
    }
    let dir = PathBuf::from(&skin_dir);
    if !dir.join("skin.toml").exists() {
        eprintln!("error: {skin_dir} has no skin.toml");
        return ExitCode::FAILURE;
    }

    // Shared, single-thread-owned host state.
    let values: Values = Default::default();
    let action_log: ActionLog = Default::default();

    // Engine-thread inbox.
    let (engine_tx, engine_rx) = mpsc::channel::<EngineMsg>();

    // HTTP + WS servers.
    let ports = serve(port, engine_tx.clone());
    let url = format!("http://127.0.0.1:{}", ports.http);
    println!("carapace-preview serving {url}  (skin: {skin_dir})");

    // File watcher → Reload messages.
    spawn_watcher(dir.clone(), engine_tx.clone());

    // Best-effort browser open (macOS `open`); harmless if it fails.
    let _ = std::process::Command::new("open").arg(&url).spawn();

    // Engine loop runs on THIS (main) thread — Engine is !Send.
    run_engine_loop(dir, values, action_log, engine_rx);
    ExitCode::SUCCESS
}

fn spawn_watcher(dir: PathBuf, engine_tx: mpsc::Sender<EngineMsg>) {
    use notify::{RecursiveMode, Watcher};
    std::thread::spawn(move || {
        let (tx, rx) = mpsc::channel();
        let mut watcher = match notify::recommended_watcher(move |res| {
            let _ = tx.send(res);
        }) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("watch disabled: {e}");
                return;
            }
        };
        if watcher.watch(&dir, RecursiveMode::Recursive).is_err() {
            eprintln!("watch disabled for {}", dir.display());
            return;
        }
        // Coalesce bursts: on any event, send a single Reload.
        while let Ok(ev) = rx.recv() {
            if ev.is_ok() {
                // Drain any queued events so a save = one reload.
                while rx.try_recv().is_ok() {}
                if engine_tx.send(EngineMsg::Reload).is_err() {
                    return;
                }
            }
        }
    });
}

fn run_engine_loop(
    dir: PathBuf,
    values: Values,
    action_log: ActionLog,
    engine_rx: mpsc::Receiver<EngineMsg>,
) {
    let mut session = SkinSession::new(dir, values.clone(), action_log.clone());

    let gpu = render::init_gpu();
    let mut renderer = carapace::render::Renderer::new(&gpu.device);
    let mut off = render::new_offscreen(
        &gpu.device,
        session.canvas.0.max(1),
        session.canvas.1.max(1),
    );
    let mut render_size = session.canvas;

    let mut clients: Vec<mpsc::Sender<OutMsg>> = Vec::new();
    let mut last_hash: Option<u64> = None;
    let mut last_png: Option<Vec<u8>> = None;
    let mut clock = Instant::now();

    loop {
        // 1. Drain inbound messages (non-blocking).
        while let Ok(msg) = engine_rx.try_recv() {
            match msg {
                EngineMsg::ClientConnected(tx) => {
                    // Greet: meta, current error state, last frame.
                    let _ = tx.send(OutMsg::Meta {
                        name: session.name.clone(),
                        w: render_size.0,
                        h: render_size.1,
                    });
                    let _ = tx.send(OutMsg::Error {
                        message: session.last_error.clone(),
                    });
                    let _ = tx.send(OutMsg::Params {
                        json: session.params_json(),
                    });
                    if let Some(png) = &last_png {
                        let _ = tx.send(OutMsg::Frame(png.clone()));
                    }
                    clients.push(tx);
                }
                EngineMsg::Client(ClientMsg::Pointer { x, y }) => {
                    if let Some(engine) = session.engine.as_mut() {
                        engine.handle_pointer_resolved(
                            render_size.0 as f32,
                            render_size.1 as f32,
                            Pt { x, y },
                            PointerEvent::Press,
                        );
                        engine.update(Duration::ZERO); // drain enqueued host action → log
                    }
                }
                EngineMsg::Client(ClientMsg::SetValue { key, value }) => {
                    if let Some(sv) = json_to_state(&value) {
                        values.borrow_mut().insert(key, sv);
                        last_hash = None; // force a resend
                    }
                }
                EngineMsg::Client(ClientMsg::RemoveValue { key }) => {
                    // Drop the key so the binding falls back to its default (get → None).
                    values.borrow_mut().remove(&key);
                    last_hash = None; // force a resend
                }
                EngineMsg::Client(ClientMsg::SetCanvas { w, h }) => {
                    let (w, h) = (w.max(1), h.max(1));
                    render_size = (w, h);
                    off = render::new_offscreen(&gpu.device, w, h);
                    last_hash = None;
                    broadcast(
                        &mut clients,
                        &OutMsg::Meta {
                            name: session.name.clone(),
                            w,
                            h,
                        },
                    );
                }
                EngineMsg::Client(ClientMsg::Pick { x, y }) => {
                    if let Some(info) =
                        session.pick(render_size.0 as f32, render_size.1 as f32, Pt { x, y })
                    {
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
                EngineMsg::Reload => {
                    session.reload();
                    // If reload succeeded and the skin's canvas changed, resync
                    // render_size/off to it before telling clients anything, so the
                    // Meta broadcast below always reflects the size we actually render.
                    if session.engine.is_some() && session.canvas != render_size {
                        render_size = (session.canvas.0.max(1), session.canvas.1.max(1));
                        off = render::new_offscreen(&gpu.device, render_size.0, render_size.1);
                    }
                    broadcast(
                        &mut clients,
                        &OutMsg::Error {
                            message: session.last_error.clone(),
                        },
                    );
                    broadcast(
                        &mut clients,
                        &OutMsg::Meta {
                            name: session.name.clone(),
                            w: render_size.0,
                            h: render_size.1,
                        },
                    );
                    broadcast(
                        &mut clients,
                        &OutMsg::Params {
                            json: session.params_json(),
                        },
                    );
                    last_hash = None;
                }
            }
        }

        // 2. Drain the action log → broadcast.
        {
            let mut log = action_log.borrow_mut();
            for action in log.drain(..) {
                broadcast(&mut clients, &OutMsg::ActionLog { action });
            }
        }

        // 3. Render only when someone is watching and a skin is loaded.
        if !clients.is_empty() {
            if let Some(engine) = session.engine.as_mut() {
                let dt = clock.elapsed();
                clock = Instant::now();
                let rgba = render::render_rgba(engine, &mut renderer, &gpu, &off, dt);
                let h = render::frame_hash(&rgba);
                if last_hash != Some(h) {
                    last_hash = Some(h);
                    let png = render::encode_png(&rgba, off.w, off.h);
                    broadcast(&mut clients, &OutMsg::Frame(png.clone()));
                    last_png = Some(png);
                }
            }
        } else {
            clock = Instant::now(); // reset dt so animation doesn't jump after reconnect
        }

        std::thread::sleep(Duration::from_millis(16)); // ~60fps ceiling
    }
}

/// Broadcast to all clients, pruning any whose receiver has dropped.
fn broadcast(clients: &mut Vec<mpsc::Sender<OutMsg>>, msg: &OutMsg) {
    clients.retain(|tx| tx.send(msg.clone()).is_ok());
}

/// Render a JSON scalar as the Lua literal text to splice into the source. Numbers pass through;
/// strings become quoted Lua strings; booleans become `true`/`false`.
fn json_scalar_to_lua(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::String(s) => lua_string_literal(s),
        _ => "nil".to_string(),
    }
}

/// Build a valid Lua double-quoted string literal for `s`. Unlike Rust's `Debug` formatting
/// (which uses `\u{...}` escapes Lua doesn't understand), this only ever emits escapes Lua
/// itself accepts: the standard backslash escapes for `\`, `"`, `\n`, `\r`, `\t`, and decimal
/// (`\ddd`) escapes for any other control character. Everything else, including printable
/// non-ASCII/UTF-8, passes through unchanged.
fn lua_string_literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\{}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn json_to_state(v: &serde_json::Value) -> Option<StateValue> {
    match v {
        serde_json::Value::Number(n) => n.as_f64().map(|f| StateValue::Scalar(f as f32)),
        serde_json::Value::String(s) => Some(StateValue::Str(Arc::from(s.as_str()))),
        serde_json::Value::Bool(b) => Some(StateValue::Bool(*b)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_scalar_to_lua_number() {
        assert_eq!(json_scalar_to_lua(&serde_json::json!(12)), "12");
    }

    #[test]
    fn json_scalar_to_lua_bool() {
        assert_eq!(json_scalar_to_lua(&serde_json::json!(true)), "true");
    }

    #[test]
    fn json_scalar_to_lua_plain_string() {
        assert_eq!(json_scalar_to_lua(&serde_json::json!("door")), "\"door\"");
    }

    #[test]
    fn json_scalar_to_lua_escapes_quote_and_backslash() {
        assert_eq!(
            json_scalar_to_lua(&serde_json::json!("a\"b\\c")),
            "\"a\\\"b\\\\c\""
        );
    }

    #[test]
    fn json_scalar_to_lua_escapes_newline() {
        let out = json_scalar_to_lua(&serde_json::json!("a\nb"));
        assert_eq!(out, "\"a\\nb\"");
        assert!(!out.contains('\n'), "newline must be escaped, not literal");
    }

    #[test]
    fn json_scalar_to_lua_escapes_control_char_as_decimal() {
        // Byte 7 (bell) must become the valid-Lua decimal escape `\7`, not Rust's
        // Debug-style `\u{7}` (which Lua does not understand).
        let out = json_scalar_to_lua(&serde_json::json!("a\u{7}b"));
        assert_eq!(out, "\"a\\7b\"");
    }
}
