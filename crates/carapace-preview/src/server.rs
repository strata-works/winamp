//! HTTP viewer server + WebSocket duplex for carapace-preview.
//! Serves one static viewer page (assets/index.html, with the live WS port templated
//! in) and pumps frames down / edits up over a per-connection WebSocket thread. The
//! engine thread talks to these server threads only through `std::sync::mpsc` — nothing
//! `!Send` crosses.
//! Consumed by the engine wiring added in a later task — kept ungated so it's testable now.

use crate::protocol::{ClientMsg, OutMsg, out_to_ws, parse_client_msg};
use std::net::TcpListener;
use std::sync::mpsc::{Receiver, Sender};
use std::time::Duration;

/// Messages delivered to the single-threaded engine loop.
pub enum EngineMsg {
    ClientConnected(Sender<OutMsg>),
    Client(ClientMsg),
    Reload,
}

pub struct Ports {
    pub http: u16,
    /// Not read by `main.rs` today — the browser learns the WS port from the templated
    /// HTML page, not from this struct. Kept for tests/tools that want it directly.
    #[allow(dead_code)]
    pub ws: u16,
}

/// Bind the HTTP viewer server + the WebSocket acceptor, spawn their loops, return the ports.
pub fn serve(http_port: u16, engine_tx: Sender<EngineMsg>) -> Ports {
    // WebSocket acceptor on an ephemeral loopback port.
    let ws_listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind ws port");
    let ws_port = ws_listener.local_addr().unwrap().port();

    // HTTP server for the single viewer page (page carries the live ws port).
    let http = tiny_http::Server::http(("127.0.0.1", http_port)).expect("bind http port");
    let bound_http = http.server_addr().to_ip().unwrap().port();
    let page = render_index(ws_port);

    // HTTP accept loop.
    std::thread::spawn(move || {
        for req in http.incoming_requests() {
            let resp = tiny_http::Response::from_string(page.clone()).with_header(
                tiny_http::Header::from_bytes(
                    &b"Content-Type"[..],
                    &b"text/html; charset=utf-8"[..],
                )
                .unwrap(),
            );
            let _ = req.respond(resp);
        }
    });

    // WS accept loop — one thread per browser connection.
    std::thread::spawn(move || {
        for stream in ws_listener.incoming().flatten() {
            let tx = engine_tx.clone();
            std::thread::spawn(move || ws_connection(stream, tx));
        }
    });

    Ports {
        http: bound_http,
        ws: ws_port,
    }
}

/// One browser connection: full-duplex pump over a single blocking socket with a
/// short read timeout (so we can interleave outbound frames without splitting the stream).
fn ws_connection(stream: std::net::TcpStream, engine_tx: Sender<EngineMsg>) {
    let mut ws = match tungstenite::accept(stream) {
        Ok(ws) => ws,
        Err(_) => return,
    };
    // After the handshake, make reads time out so the loop can also write.
    let _ = ws
        .get_ref()
        .set_read_timeout(Some(Duration::from_millis(10)));

    let (out_tx, out_rx): (Sender<OutMsg>, Receiver<OutMsg>) = std::sync::mpsc::channel();
    if engine_tx.send(EngineMsg::ClientConnected(out_tx)).is_err() {
        return;
    }

    loop {
        // 1. Drain everything the engine wants to send this client.
        let mut engine_gone = false;
        loop {
            match out_rx.try_recv() {
                Ok(msg) => {
                    if ws.send(out_to_ws(&msg)).is_err() {
                        return; // socket dead
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    engine_gone = true;
                    break;
                }
            }
        }
        if engine_gone {
            return;
        }

        // 2. Read one inbound message (or time out).
        match ws.read() {
            Ok(tungstenite::Message::Text(t)) => {
                if let Ok(cm) = parse_client_msg(t.as_str())
                    && engine_tx.send(EngineMsg::Client(cm)).is_err()
                {
                    return;
                }
            }
            Ok(tungstenite::Message::Close(_)) => return,
            Ok(_) => {} // ping/pong/binary from browser: ignore
            Err(tungstenite::Error::Io(e))
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut => {}
            Err(_) => return,
        }
    }
}

pub fn render_index(ws_port: u16) -> String {
    include_str!("../assets/index.html").replace("{{WS_PORT}}", &ws_port.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_the_ws_port_into_the_page() {
        let html = render_index(54321);
        assert!(html.contains("54321"));
        assert!(!html.contains("{{WS_PORT}}"));
    }
}
