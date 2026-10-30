# Mercury

A Rust reimagining of [gold](https://github.com/) — a framework whose thesis is
*high-level, JavaScript-like ergonomics for prototyping without sacrificing
speed*. Mercury carries that thesis forward in Rust, where the type system lets
it be safer *and* friendlier.

Mercury is a standalone project. It is **not** a gold branch. Gold (the C++
codebase) is untouched by this repo.

> **Name:** gold is **Au**, Mercury is **Hg** — both chemical-element metals.
> The pairing is a joke *and* a signal.

## The idea

gold's unique move was adding accessible, JS-like syntax on top of a fast core.
Mercury does the same, but honest about where Rust can and can't copy
JavaScript:

- **Faithful ports:** the `Var` value, prototype-based objects, ordered lists,
  the `o!`/`a!` literal macros, template strings, array helpers.
- **Rust-native replacements for what can't copy:** explicit `.get()`/`.put()`
  instead of implicit assignment-conversion; typed reads via `Into<…>`; a `Null`
  literal. These trade a footgun for borrow-checked safety.
- **Better than gold:** serde serialization for free, and (planned) derive
  macros that turn a `Var` into a typed struct for the hot path.

## Quick start

```rust
use mercury::{a, o, Null, Var};

// object / array literals read like JS
let user = o! {
    "name": "bob",
    "age": 30i64,
    "tags": a!["admin", "dev"],
    "stats": o!{ "score": 9i64 },
};

// write through the shared object (gold's v["k"] = ... semantics)
user.at("stats").at("score").put(10i64);
assert_eq!(user.get_path("stats.score").as_i64(), Some(10));

// typed read
let name: String = user.get("name").into();
assert_eq!(name, "bob");

// null is a literal
let missing: Var = user.get("nope");
assert_eq!(missing, Null);
```

Serde round-trips through any format (JSON shown):

```rust
let v: Var = serde_json::from_str(r#"{"a":[1,2,3],"b":true}"#).unwrap();
let json = serde_json::to_string(&v).unwrap();
```

## Modules

- **Core** — `Var` / `Object` / `List` / `VarRef` / sugar / serde / `Func`. *Done.*
- **Web** — gold's expressjs-like `Server`: all verbs, `:param`/`*` patterns,
  WebSocket routes, static mounts, an error hook, graceful start/stop
  (axum + tokio, behind the `web` feature) — plus the always-on `html`
  builder/parser, `css` parser, and file-backed `store`. *Done except
  streaming responses and HTTPS.*
- **Game** — `wgpu` graphics, pump-and-dispatch engine loop. Planned.

See [docs/roadmap.md](docs/roadmap.md) for the full plan and the architecture
decisions (why `Arc<Mutex>`, why serde, why not Bevy).

## Build & test

```
cargo test                  # core tests
cargo test --all-features   # + web layer
cargo clippy --all-targets
```

## License

MIT
