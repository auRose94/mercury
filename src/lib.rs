//! # Mercury
//!
//! A Rust reimagining of **gold** — a framework whose thesis is *high-level,
//! JavaScript-like ergonomics for prototyping without sacrificing speed*.
//!
//! Mercury ports gold's core (`var`/`object`/`list` with prototype
//! inheritance) and its sugar (`o!`, `a!`, `tpl!`, array helpers), and
//! improves on it where Rust lets us: explicit typed reads are borrow-checked
//! and footgun-free, and the shared-data model makes the `VarRef` proxy
//! mutate through handles the way gold does.
//!
//! ```
//! use mercury::{o, Var};
//!
//! let user = o!{
//!     "name": "bob",
//!     "age": 30i64,
//!     "tags": mercury::a!["admin", "dev"],
//!     "stats": o!{ "score": 9i64 },
//! };
//!
//! // write through the shared object (gold's v["k"] = ... semantics)
//! user.at("stats").at("score").put(10i64);
//! assert_eq!(user.get_path("stats.score").as_i64(), Some(10));
//!
//! // typed read
//! let name: String = user.get("name").into();
//! assert_eq!(name, "bob");
//! ```

pub mod list;
pub mod object;
pub mod proxy;
pub mod sugar;
pub mod var;

pub use list::List;
pub use object::Object;
pub use proxy::VarRef;
pub use var::{Null, Quat, Ty, Var, Vec3, Vec4};
