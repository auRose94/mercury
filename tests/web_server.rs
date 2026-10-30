//! Server-facade tests: real sockets, real HTTP/WebSocket wire traffic —
//! verbs, `:param`/`*` patterns, request staging (query/form/json/headers),
//! response controls, mounts, the error hook, WebSockets, and the graceful
//! stop/restart lifecycle. Every server binds port `0` (ephemeral) so tests
//! can run in parallel.

#![cfg(feature = "web")]

use std::time::Duration;

use mercury::web::{req_header, req_is_json, Response, Server, WsHandlers};
use mercury::{o, Var};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

// ---- tiny HTTP/1.1 test client (one request per connection; `connection:
// close` makes read-to-end the end of the body) ----

async fn try_http(addr: &str, raw: &str) -> Option<(u16, String, String)> {
    let mut s = TcpStream::connect(addr).await.ok()?;
    s.write_all(raw.as_bytes()).await.ok()?;
    let mut buf = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), s.read_to_end(&mut buf))
        .await
        .ok()?
        .ok()?;
    let text = String::from_utf8_lossy(&buf).into_owned();
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
    let status: u16 = head
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    Some((status, head.to_string(), body.to_string()))
}

async fn http(addr: &str, raw: &str) -> (u16, String, String) {
    try_http(addr, raw).await.expect("request failed")
}

fn get(path: &str) -> String {
    format!("GET {path} HTTP/1.1\r\nhost: t\r\nconnection: close\r\n\r\n")
}

fn request(method: &str, path: &str) -> String {
    format!("{method} {path} HTTP/1.1\r\nhost: t\r\nconnection: close\r\n\r\n")
}

fn post(path: &str, ct: &str, body: &str) -> String {
    format!(
        "POST {path} HTTP/1.1\r\nhost: t\r\ncontent-type: {ct}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    )
}

/// Spawn `start()` on a clone (clones share the stop signal), wait for the
/// bind, return the resolved addr.
async fn boot(serv: &Server) -> String {
    let s = serv.clone();
    tokio::spawn(async move { let _ = s.start().await; });
    for _ in 0..200 {
        let addr = serv.local_addr();
        if !addr.is_empty() {
            return addr;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("server did not bind in time");
}

fn ephemeral() -> Server {
    Server::with_config(o!{ "host": "127.0.0.1", "port": 0 })
}

// ---- routing: verbs, params, wildcards ----

#[tokio::test(flavor = "multi_thread")]
async fn verbs_and_params() {
    let mut serv = ephemeral();
    serv.get("/users/:id", |req| {
        let id = req.get("params").get("id").as_string().unwrap_or_default();
        Response::json(o!{ "id": id })
    })
    .post("/users", |req| Response::json(req.get("body")))
    .put("/things/:tid", |req| {
        Var::from(req.get("params").get("tid").as_string().unwrap_or_default())
    })
    .patch("/things/:tid", |_req| "patched")
    .delete("/things/:tid", |_req| Response::text("deleted").status(204))
    .options("/things/:tid", |_req| Response::text("ok"));
    let addr = boot(&serv).await;

    let (st, _, body) = http(&addr, &get("/users/7")).await;
    assert_eq!((st, body.as_str()), (200, r#"{"id":"7"}"#));

    // path params are percent-decoded
    let (st, _, body) = http(&addr, &get("/users/alice%20smith")).await;
    assert_eq!((st, body.as_str()), (200, r#"{"id":"alice smith"}"#));

    let (st, _, body) = http(&addr, &post("/users", "application/json", r#"{"n":1}"#)).await;
    assert_eq!((st, body.as_str()), (200, r#"{"n":1}"#));

    let (st, _, body) = http(&addr, &request("PUT", "/things/2")).await;
    assert_eq!((st, body.as_str()), (200, r#""2""#));

    let (st, _, body) = http(&addr, &request("PATCH", "/things/2")).await;
    assert_eq!((st, body.as_str()), (200, "patched"));

    let (st, _, body) = http(&addr, &request("OPTIONS", "/things/2")).await;
    assert_eq!((st, body.as_str()), (200, "ok"));

    let (st, _, body) = http(&addr, &request("DELETE", "/things/2")).await;
    // 204 No Content carries no body, per HTTP
    assert_eq!((st, body.as_str()), (204, ""));
}

#[tokio::test(flavor = "multi_thread")]
async fn wildcard_mount_and_error_fallthrough() {
    let tmp = std::env::temp_dir().join("mercury_web_mount_test");
    std::fs::create_dir_all(tmp.join("css")).unwrap();
    std::fs::write(tmp.join("css/site.css"), "body{color:red}").unwrap();

    let mut serv = ephemeral();
    serv.get("/files/*", |req| {
        let rest = req.get("params").get("*").as_string().unwrap_or_default();
        Var::from(format!("got:{rest}"))
    })
    .mount("/static", &tmp)
    .on_error(|req| {
        let path = req.get("path").as_string().unwrap_or_default();
        Response::text(format!("404:{path}")).status(404)
    });
    let addr = boot(&serv).await;

    // the trailing wildcard captures slashes, JS-style
    let (st, _, body) = http(&addr, &get("/files/css/site.css")).await;
    assert_eq!((st, body.as_str()), (200, r#""got:css/site.css""#));

    // the mount serves the file, with the right content type
    let (st, head, body) = http(&addr, &get("/static/css/site.css")).await;
    assert_eq!(st, 200);
    assert_eq!(body, "body{color:red}");
    assert!(head.to_ascii_lowercase().contains("content-type: text/css"));

    // a mount miss falls through to the error hook (gold's ordering)
    let (st, _, body) = http(&addr, &get("/static/nope.css")).await;
    assert_eq!((st, body.as_str()), (404, "404:/static/nope.css"));

    // so does a fully unrouted path
    let (st, _, body) = http(&addr, &get("/nowhere")).await;
    assert_eq!((st, body.as_str()), (404, "404:/nowhere"));
}

// ---- request staging: query, form, json, headers ----

#[tokio::test(flavor = "multi_thread")]
async fn request_staging() {
    let mut serv = ephemeral();
    serv.get("/q", |req| {
        let who = req.get("query").get("who").as_string().unwrap_or_default();
        let method = req.get("method").as_string().unwrap_or_default();
        Var::from(format!("{method}:{who}:{}", req_header(&req, "x-tag")))
    })
    .post("/form", |req| {
        assert!(mercury::web::req_is_www_form_urlencoded(&req));
        Var::from(req.get("body").get("name").as_string().unwrap_or_default())
    })
    .post("/api", |req| {
        assert!(req_is_json(&req));
        Response::json(o!{
            "n": req.get("body").get("n").as_i64().unwrap_or(0),
        })
    });
    let addr = boot(&serv).await;

    // query pairs decoded, headers staged lowercased
    let (st, _, body) = http(
        &addr,
        "GET /q?who=alice%20smith&x=1 HTTP/1.1\r\nhost: t\r\nx-tag: t1\r\nconnection: close\r\n\r\n",
    )
    .await;
    assert_eq!((st, body.as_str()), (200, r#""GET:alice smith:t1""#));

    // + means space in form bodies
    let (st, _, body) = http(
        &addr,
        &post("/form", "application/x-www-form-urlencoded", "name=bob+marley&x=1"),
    )
    .await;
    assert_eq!((st, body.as_str()), (200, r#""bob marley""#));

    let (st, _, body) = http(&addr, &post("/api", "application/json", r#"{"n":3}"#)).await;
    assert_eq!((st, body.as_str()), (200, r#"{"n":3}"#));
}

// ---- response controls: status, headers, body types ----

#[tokio::test(flavor = "multi_thread")]
async fn response_status_headers_and_types() {
    let mut serv = ephemeral();
    serv.get("/json", |_req| {
        Response::json(o!{ "ok": true }).status(201).header("x-a", "b")
    })
    .get("/html", |_req| "plain string → html")
    .get("/raw", |_req| Response::bytes(vec![1, 2, 3]).ct("application/x-thing"));
    let addr = boot(&serv).await;

    let (st, head, body) = http(&addr, &get("/json")).await;
    assert_eq!(st, 201);
    assert_eq!(body, r#"{"ok":true}"#);
    let h = head.to_ascii_lowercase();
    assert!(h.contains("content-type: application/json"), "{h}");
    assert!(h.contains("x-a: b"), "{h}");

    let (st, head, body) = http(&addr, &get("/html")).await;
    assert_eq!(st, 200);
    assert_eq!(body, "plain string → html");
    assert!(head.to_ascii_lowercase().contains("text/html"), "{}", head);

    let (_, head, _) = http(&addr, &get("/raw")).await;
    assert!(
        head.to_ascii_lowercase().contains("application/x-thing"),
        "{}",
        head
    );
}

// ---- WebSockets (gold's ws + wsSocket) ----

#[tokio::test(flavor = "multi_thread")]
async fn websocket_echo() {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message as WsMsg;

    let mut serv = ephemeral();
    serv.ws(
        "/echo",
        WsHandlers::new()
            .on_open(|sock| {
                sock.send("welcome");
            })
            .on_message(|sock, data| {
                sock.send(data.as_string().unwrap_or_default());
            })
            .on_close(|_sock| {}),
    );
    let addr = boot(&serv).await;

    let (mut ws, _) =
        tokio_tungstenite::connect_async(format!("ws://{addr}/echo")).await.unwrap();

    let msg = tokio::time::timeout(Duration::from_secs(5), ws.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(msg, WsMsg::Text("welcome".into()));

    ws.send(WsMsg::text("hi there")).await.unwrap();
    let msg = tokio::time::timeout(Duration::from_secs(5), ws.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(msg, WsMsg::Text("hi there".into()));

    ws.close(None).await.unwrap();
}

// ---- lifecycle: graceful stop, restart (gold's restartable server) ----

#[tokio::test(flavor = "multi_thread")]
async fn graceful_stop_and_restart() {
    let mut serv = ephemeral();
    serv.get("/", |_req| "up");

    let s = serv.clone();
    let task = tokio::spawn(async move { s.start().await });
    let addr = boot(&serv).await;

    let (st, _, body) = http(&addr, &get("/")).await;
    assert_eq!((st, body.as_str()), (200, "up"));

    assert!(serv.stop());
    // start() returns once the loop exits; a second stop has nothing to stop
    assert_eq!(task.await.unwrap().unwrap(), ());
    assert!(!serv.stop());

    // restart in the same process — gold's server loop story
    let s = serv.clone();
    tokio::spawn(async move { let _ = s.start().await; });
    for _ in 0..200 {
        let addr = serv.local_addr();
        if !addr.is_empty() {
            if let Some((st, _, body)) = try_http(&addr, &get("/")).await {
                if st == 200 && body == "up" {
                    break;
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let addr = serv.local_addr();
    let (st, _, body) = http(&addr, &get("/")).await;
    assert_eq!((st, body.as_str()), (200, "up"));
    serv.stop();
}
