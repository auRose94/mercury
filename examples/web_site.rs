//! A multi-page homage to gold's `myWebProject` example: shared state,
//! pages with a navbar component, form handling, a JSON API, path params,
//! query strings, WebSocket chat, static assets, and styled 404s — all over
//! `Var`.
//!
//! Run:
//!   cargo run --example web_site --features web
//!
//! Then: http://127.0.0.1:8377 (users, login form, chat, /api/*, a 404 page).

use mercury::web::{Response, Server, WsHandlers};
use mercury::{o, Var};

// ---- tiny HTML helpers (until Mercury grows gold's HTML builder module) ----

fn page(title: &str, body: &str) -> Response {
    Response::html(format!(
        r#"<!DOCTYPE html><html lang="en"><head><meta charset="utf-8">
<title>{title} — Hg</title><link rel="stylesheet" href="/assets/site.css"></head>
<body>{body}</body></html>"#,
    ))
}

fn navbar(active: &str) -> String {
    let link = |href: &str, label: &str| {
        let cls = if href == active { "active" } else { "" };
        format!(r#"<a class="{cls}" href="{href}">{label}</a>"#)
    };
    format!(
        r#"<nav><strong>Hg</strong> {} {} {}</nav>"#,
        link("/", "home"),
        link("/users", "users"),
        link("/chat", "chat"),
    )
}

const DEFAULT_CSS: &str = r#"
:root { color-scheme: dark; }
body { font-family: system-ui, sans-serif; margin: 0; background: #14161a; color: #e8e6e3; }
nav { display: flex; gap: 1rem; padding: 1rem 1.5rem; background: #1d2026; border-bottom: 1px solid #2a2e36; }
nav a { color: #9aa3b2; text-decoration: none; padding: .2rem .5rem; border-radius: 6px; }
nav a.active, nav a:hover { color: #fff; background: #2a2e36; }
main { max-width: 40rem; margin: 2rem auto; padding: 0 1rem; }
h1 { font-weight: 600; } code, pre { background: #1d2026; border-radius: 6px; }
code { padding: .1rem .35rem; } pre { padding: 1rem; overflow-x: auto; }
label { display: block; margin: 1rem 0 .3rem; }
input { background: #1d2026; color: inherit; border: 1px solid #2a2e36; border-radius: 6px; padding: .5rem .75rem; }
button { background: #4f6df5; color: #fff; border: 0; border-radius: 6px; padding: .5rem 1rem; margin-top: 1rem; }
.error { color: #ff7b72; } .card { background: #1d2026; border: 1px solid #2a2e36; border-radius: 10px; padding: 1rem 1.25rem; margin: 1rem 0; }
"#;

fn ensure_assets() {
    std::fs::create_dir_all("public").ok();
    let css = std::path::Path::new("public/site.css");
    if !css.exists() {
        std::fs::write(css, DEFAULT_CSS).ok();
    }
}

#[tokio::main]
async fn main() {
    ensure_assets();

    // The "database": one shared Var handle. Each route captures its own
    // clone — writes through any of them are visible everywhere.
    let users = Var::from(o!{
        "1": o!{ "name": "alice", "role": "admin" },
        "2": o!{ "name": "bob", "role": "user" },
    });
    let users_home = users.clone();
    let users_list = users.clone();
    let users_detail = users.clone();
    let users_api = users.clone();

    let mut serv = Server::with_config(o!{ "host": "127.0.0.1", "port": 8377 });

    serv
        // pages
        .get("/", move |_req| {
            page(
                "home",
                &format!(
                    "{}<main><h1>Mercury web</h1><p>Gold's expressjs-like server, in Rust: routes are Var closures.</p><ul><li>{} registered users</li><li>try <code>/hello/mercury?shout=1</code></li><li>chat over WebSocket at <code>/chat</code></li><li>JSON API at <code>/api/users</code></li></ul></main>",
                    navbar("/"),
                    users_home.as_object().map(|o| o.len()).unwrap_or(0),
                ),
            )
        })
        .get("/login", move |_req| {
            page(
                "login",
                &format!(
                    r#"{}<main><h1>Log in</h1>
<form method="post" action="/login">
  <label for="name">Name</label><input id="name" name="name">
  <button>go</button>
</form></main>"#,
                    navbar("/login"),
                ),
            )
        })
        .post("/login", |req| {
            // form bodies arrive as a Var object, decoded
            let name = req.get("body").get("name").as_string().unwrap_or_default();
            if name.is_empty() {
                return page(
                    "login",
                    &format!(
                        r#"{}<main><h1>Log in</h1><p class="error">A name is required.</p>
<form method="post" action="/login"><label for="name">Name</label><input id="name" name="name"><button>go</button></form></main>"#,
                        navbar("/login"),
                    ),
                );
            }
            page(
                "welcome",
                &format!("{}<main><h1>Hi {}!</h1><p>Form handling, gold-style.</p></main>", navbar("/"), name),
            )
        })
        .get("/users", move |_req| {
            let mut rows = String::new();
            let entries = users_list.as_object().map(|o| o.entries()).unwrap_or_default();
            for (id, user) in entries {
                rows.push_str(&format!(
                    r#"<div class="card"><a href="/users/{id}">#{id} {}</a> — {}</div>"#,
                    user.get("name").as_string().unwrap_or_default(),
                    user.get("role").as_string().unwrap_or_default(),
                ));
            }
            page("users", &format!("{}<main><h1>Users</h1>{rows}</main>", navbar("/users")))
        })
        .get("/users/:id", move |req| {
            let id = req.get("params").get("id").as_string().unwrap_or_default();
            match users_detail.get(&id) {
                user if user.ty() == mercury::Ty::Object => page(
                    "user",
                    &format!(
                        "{}<main><h1>#{id} {}</h1><p>role: {}</p></main>",
                        navbar("/users"),
                        user.get("name").as_string().unwrap_or_default(),
                        user.get("role").as_string().unwrap_or_default(),
                    ),
                ),
                _ => Response::text("no such user").status(404),
            }
        })
        .get("/hello/:name", |req| {
            let name = req.get("params").get("name").as_string().unwrap_or_default();
            let shout = req.get("query").get("shout").as_string().unwrap_or_default() == "1";
            let greeting = format!("hello, {name}!");
            if shout {
                Var::from(greeting.to_uppercase())
            } else {
                Var::from(greeting)
            }
        })
        // JSON API
        .get("/api/users", move |_req| {
            Response::json(users_api.clone())
        })
        .post("/api/echo", |req| {
            // JSON bodies arrive as Var; malformed input arrives as Null
            let body = req.get("body");
            Response::json(o!{ "echo": body, "ok": body.ty() != mercury::Ty::Null }).status(201)
        })
        // WebSocket chat (echo)
        .ws(
            "/ws",
            WsHandlers::new()
                .on_open(|sock| {
                    sock.send(r#"{"hello": "say something and I'll echo it"}"#);
                })
                .on_message(|sock, data| {
                    sock.send(data.as_string().unwrap_or_default());
                })
                .on_close(|_sock| {}),
        )
        .get("/chat", move |_req| {
            page(
                "chat",
                &format!(
                    r#"{}<main><h1>Chat</h1><pre id="log"></pre>
<script>
const log = document.getElementById("log");
const ws = new WebSocket("ws://" + location.host + "/ws");
ws.onmessage = (e) => {{ log.textContent += e.data + "\n"; }};
document.body.addEventListener("click", () => {{ const m = prompt("message"); if (m) ws.send(m); }});
</script></main>"#,
                    navbar("/chat"),
                ),
            )
        })
        // static files: ./public served under /assets
        .mount("/assets", "public")
        // styled 404 — gold's setErrorHandler hook
        .on_error(|req| {
            let path = req.get("path").as_string().unwrap_or_default();
            page(
                "404",
                &format!(
                    r#"{}<main><h1>404</h1><p>No route or asset matches <code>{}</code>.</p></main>"#,
                    navbar(""),
                    path,
                ),
            )
            .status(404)
        });

    println!("serving on http://127.0.0.1:8377  (pages, /api/*, /chat, /assets/*)");
    serv.start().await.unwrap();
}
