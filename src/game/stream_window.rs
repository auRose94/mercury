//! The streaming window backend: the headless render buffer becomes a web
//! page. A tiny axum server serves a canvas, pushes the latest frame as a
//! binary WebSocket message, and turns browser events (keys, mouse, the
//! page's Quit button) back into `Var` events — the engine can't tell it
//! apart from a real windowing system. The page auto-opens in the browser
//! on launch, which also makes the whole loop trivially inspectable: an AI
//! (or a test) can read the frame, click, and send keys without a GUI.
//!
//! Behind the `stream` feature (reuses axum + tokio from the web layer).

use std::sync::mpsc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::{get};
use axum::Router;
use futures_util::{SinkExt, StreamExt};
use crate::game::render::FrameSink;
use crate::game::window::WindowSystem;
use crate::var::Var;

const PAGE: &str = r#"<!DOCTYPE html>
<html lang="en"><head><meta charset="utf-8"><title>Mercury — streaming window</title>
<style>
body { margin: 0; background: #14161a; color: #e8e6e3; font-family: system-ui, sans-serif;
       display: flex; flex-direction: column; align-items: center; gap: .5rem; padding: 1rem; }
canvas { image-rendering: pixelated; border: 1px solid #2a2e36; border-radius: 8px; max-width: 96vw; }
button { background: #4f6df5; color: #fff; border: 0; border-radius: 6px; padding: .4rem 1rem; }
#status { color: #9aa3b2; font-size: .85rem; }
</style></head>
<body>
<canvas id="screen" width="320" height="180"></canvas>
<div><button id="quit">quit</button><span id="status">connecting…</span></div>
<script>
const canvas = document.getElementById("screen");
const ctx = canvas.getContext("2d");
const status = document.getElementById("status");
const ws = new WebSocket(`ws://${location.host}/ws`);
ws.binaryType = "arraybuffer";
ws.onopen = () => { status.textContent = "streaming"; };
ws.onclose = () => { status.textContent = "disconnected"; };
ws.onmessage = (m) => {
  if (m.data instanceof ArrayBuffer) {
    const dv = new DataView(m.data);
    const w = dv.getUint32(0, true), h = dv.getUint32(4, true);
    if (canvas.width !== w || canvas.height !== h) { canvas.width = w; canvas.height = h; }
    const pixels = new Uint8ClampedArray(m.data, 8, w * h * 4);
    ctx.putImageData(new ImageData(pixels, w, h), 0, 0);
  } else {
    status.textContent = m.data;
  }
};
function send(ev) { if (ws.readyState === 1) ws.send(JSON.stringify(ev)); }
addEventListener("keydown", (e) => send({ type: "key", key: e.key, pressed: true }));
addEventListener("keyup",   (e) => send({ type: "key", key: e.key, pressed: false }));
function canvasPos(e) {
  const r = canvas.getBoundingClientRect();
  return { x: Math.round((e.clientX - r.left) / r.width * canvas.width),
           y: Math.round((e.clientY - r.top) / r.height * canvas.height) };
}
canvas.addEventListener("mousedown", (e) => { const p = canvasPos(e); send({ type: "mouse", action: "down", button: e.button, x: p.x, y: p.y }); });
canvas.addEventListener("mouseup",   (e) => { const p = canvasPos(e); send({ type: "mouse", action: "up", button: e.button, x: p.x, y: p.y }); });
canvas.addEventListener("mousemove", (e) => { const p = canvasPos(e); send({ type: "mouse", action: "move", x: p.x, y: p.y }); });
canvas.addEventListener("wheel",     (e) => { const p = canvasPos(e); send({ type: "wheel", delta: e.deltaY, x: p.x, y: p.y }); e.preventDefault(); }, { passive: false });
document.getElementById("quit").addEventListener("click", () => send({ type: "quit" }));
</script>
</body></html>"#;

/// The streaming window backend: serves the page, pushes frames from a
/// [`FrameSink`], and delivers browser events through
/// [`WindowSystem::poll`](crate::game::WindowSystem::poll) — keys, mouse,
/// wheel, and a page `quit` button, all as `Var` objects.
pub struct StreamingWindow {
    url: String,
    events: mpsc::Receiver<Var>,
    _server: std::thread::JoinHandle<()>,
}

impl StreamingWindow {
    /// Bind an HTTP server on `bind` (e.g. `"127.0.0.1:8400"` — port `0`
    /// picks a free one), streaming frames from `sink`. With
    /// `open_browser`, launches the system browser at the page.
    ///
    /// # Errors
    /// The bind failed.
    pub fn new(bind: &str, sink: FrameSink, open_browser: bool) -> Result<Self, String> {
        let (event_tx, event_rx) = mpsc::channel::<Var>();
        let std_listener = std::net::TcpListener::bind(bind)
            .map_err(|e| format!("stream bind {bind}: {e}"))?;
        let addr = std_listener
            .local_addr()
            .map_err(|e| format!("stream local_addr: {e}"))?;
        let url = format!("http://{addr}/");

        let handle = std::thread::Builder::new()
            .name("mercury-stream".to_string())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("stream runtime");
                // tokio's into_std left the fd blocking; from_std needs the opposite
                std_listener.set_nonblocking(true).expect("stream nonblocking");
                runtime.block_on(async move {
                    let app = Router::new()
                        .route("/", get(|| async { axum::response::Html(PAGE) }))
                        .route("/ws", get(ws_upgrade))
                        .with_state((sink, event_tx));
                    let listener = tokio::net::TcpListener::from_std(std_listener)
                        .expect("stream listener");
                    axum::serve(listener, app).await.expect("stream server");
                });
            })
            .map_err(|e| format!("stream thread: {e}"))?;

        if open_browser {
            open_in_browser(&url);
        }
        Ok(StreamingWindow { url, events: event_rx, _server: handle })
    }

    /// The page URL (useful when the browser wasn't auto-opened).
    pub fn url(&self) -> &str { &self.url }
}

impl WindowSystem for StreamingWindow {
    /// Non-blocking: drains everything the browser sent since last poll.
    fn poll(&mut self) -> Vec<Var> {
        let mut events = Vec::new();
        while let Ok(event) = self.events.try_recv() {
            events.push(event);
        }
        events
    }

    fn handle_event(&mut self, _event: &Var) {}
}

/// The WebSocket upgrade route: state carries the sink and the event pipe.
async fn ws_upgrade(
    ws: WebSocketUpgrade,
    State((sink, events)): State<(FrameSink, mpsc::Sender<Var>)>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| stream_socket(socket, sink, events))
}

/// One connected browser: push frames when they change, receive events.
async fn stream_socket(socket: WebSocket, sink: FrameSink, events: mpsc::Sender<Var>) {
    let (mut sender, mut receiver) = socket.split();
    let mut generation = 0u64;
    let mut tick = tokio::time::interval(std::time::Duration::from_millis(33));
    loop {
        tokio::select! {
            _ = tick.tick() => {
                if let Some(frame) = sink.latest_since(generation) {
                    generation = frame.generation;
                    let mut message =
                        Vec::with_capacity(8 + frame.rgba.len());
                    message.extend_from_slice(&frame.width.to_le_bytes());
                    message.extend_from_slice(&frame.height.to_le_bytes());
                    message.extend_from_slice(&frame.rgba);
                    if sender.send(Message::Binary(message.into())).await.is_err() {
                        break;
                    }
                }
            }
            incoming = receiver.next() => {
                match incoming {
                    Some(Ok(Message::Text(text))) => {
                        // browser event JSON → a Var event for the engine
                        if let Ok(event) = serde_json::from_str::<Var>(&text) {
                            if events.send(event).is_err() {
                                break;
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
        }
    }
}

fn open_in_browser(url: &str) {
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd").args(["/c", "start", url]).spawn();
}
