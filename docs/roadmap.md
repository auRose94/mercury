# Mercury roadmap

Mercury is a standalone Rust project: a reimagining of **gold** (the C++20/26
framework) whose thesis is *high-level, JavaScript-like ergonomics for
prototyping without sacrificing speed*. Mercury is **not** a gold branch — it
lives in its own repo and is free to diverge. Gold stays untouched.

The name is a deliberate joke and a real signal: gold is **Au**, Mercury is
**Hg** — both are chemical-element metals, so the pairing keeps the parallel.
Mercury is also the swift messenger, fitting the "fast prototyping" goal.

## Core thesis

gold's unique idea is adding accessible, JS-like syntax on top of a fast core.
Mercury carries that forward but is honest about where Rust can and can't
copy JavaScript:

- **What ports faithfully:** the `var` value, prototype-based objects,
  ordered lists, the `o!`/`a!` literal macros, template strings, and the
  array helpers.
- **What can't port (and the Rust-native replacement):**
  - gold's implicit `v["k"] = 5` assignment-conversion → explicit
    `.get()` / `.put()` / `.set()`. This trades a footgun for borrow-checked
    safety — the chaos gold's implicit casts caused is unrepresentable here.
  - gold's `varRef::operator=` → the `VarRef` proxy's `.put()`.
- **What's better in Rust:** explicit typed reads via `Into<…>` / `as_*()`,
  a `Null` literal instead of a verbose type tag, and (planned) `#[derive]`
  traits and serde — the type system does work gold had to paper over.

## Current state — Core module

The core is implemented and tested (`tests/ergonomics.rs`, `tests/serde.rs`):

| File | What |
| --- | --- |
| `src/var.rs` | `Var` — the COW dynamic value; `Ty` tag; arithmetic; `From` conversions; `Null` literal; serde `Serialize`/`Deserialize` |
| `src/object.rs` | `Object` — insertion-ordered map + prototype chain (`set_parent`/`inherits`); serde |
| `src/list.rs` | `List` — ordered array, grows-with-nulls on index-set |
| `src/proxy.rs` | `VarRef` — write-back proxy (`v.at("k").put(5)`), mutates the shared value in place |
| `src/sugar.rs` | `o!{}`, `a![]`, `tpl!`, `each`/`map`/`filter`/`find`/`join` |

Key design choices:

- **`Rc<RefCell<…>>` payloads.** This is what lets the `VarRef` proxy mutate
  the *original* shared value in place — the faithful gold behavior. It also
  means single-threaded safety comes from the borrow checker; a multithreaded
  build would swap `RefCell` for `Mutex`.
- **serde is the non-negotiable serialization layer.** One `Serialize`/
  `Deserialize` impl on `Var` replaces gold's hand-rolled JSON/BSON/CBOR/
  MsgPack/UBJSON — any serde format works for free. The `Func` variant has no
  data-model form, so it serializes as `null` (you can't round-trip a closure).
- **`o!`/`a!`** (renamed from `jo!`/`ja!`): `o` reads as "object" instantly;
  the `j` was only there to disambiguate for C++/HTML keywords.

## Where Mercury goes from here

The plan follows gold's own module structure, adapted to Rust's ecosystem
rather than reinventing wheels.

### 1. Core (done) → harden

- wgpu-free today; the core is pure data. Next: a `FromVar` derive macro —
  the Rust-native answer to "script ergonomics + native speed": derive a
  typed struct from a `Var` for the hot path.
- `Var` equality treats two functions as unequal (closures have no inspectable
  identity); revisit if a real use case needs it.

### 2. Web module

gold had a web/UI layer; Mercury's web layer leans on Rust's mature HTTP
ecosystem instead of reinventing it:

- **`hyper`** (or `axum`, which is built on hyper) for the HTTP server — the
  wheel for request/response handling.
- **`socket2`** underneath where we need low-level socket control that hyper
  doesn't expose.
- **`tokio` + `futures`** for async. Mercury's data flows through `Var`, so a
  request handler is just a function `Var -> Var` (input params → response),
  which composes beautifully with async: `async fn handler(req: Var) -> Var`.
- The goal: an expressjs-like surface where routes are JS-style closures over
  `Var`, not typed request/response structs.

### 3. Game module

- **`wgpu`** for graphics — deliberately *not* bgfx. wgpu is the modern Rust
  native, cross-platform, WebGPU-adjacent choice. Targeting one backend means
  we never maintain a dozen graphics backends again.
- The engine loop stays a plain pump + dispatch (gold's design): events,
  handlers, and results cross boundaries as `Var`, so there's no platform
  switch in the loop.
- Physics/other subsystems follow gold's registered-backend pattern but as
  Rust traits + `register`/`create` registries.

### Scripting layer (cross-cutting)

- **DevYatsu's `script` crate** (a TypeScript-like interpreter) is the natural
  scripting host — and DevYatsu also made an expressjs-like web lib, so it
  pairs with the web module. Mercury's `Var` is the bridge between native Rust
  and scripts.
- The ambition: write a prototype in `Var`/script ergonomics, then derive a
  typed native path for the hot loop. That's gold's thesis, made Rust-native.

## What Mercury will NOT do

- **Not integrate into Bevy.** Bevy is ECS-first (homogeneous typed components,
  archetype storage); `Var` is type-erased and shared-mutable. Forcing `Var`
  into ECS systems fights the borrow checker and throws away both systems'
  strengths. Mercury is the scripting/data layer; others may *choose* to bridge
  it to Bevy later, but Mercury doesn't depend on or bend for Bevy.
- **Not reinvent wheels.** serde (serialization), hyper (HTTP), wgpu (graphics),
  tokio/futures (async), DevYatsu's script (interpreter) — each is the right
  existing tool. Mercury's value is the `Var` seam and the ergonomics, not
  reimplementing the Rust ecosystem.

## Verify

```
cargo test        # core + serde tests
cargo clippy
```
