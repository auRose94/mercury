//! The JS-like sugar — the reason this exists.
//!
//! - `o!{ "k": v, ... }` builds an `Object` (JS object literal)
//! - `a![a, b, c]` builds a `List` (JS array literal)
//! - `tpl!("...$0 $1", a, b)` builds a string (JS template literals)
//!
//! `o!`/`a!` nest, exactly like gold's `jo(...)`/`ja(...)`. These are the
//! idioms that make prototyping feel like JavaScript.

use crate::list::List;
use crate::var::Var;

/// Build an object literal. Keys are strings, values are any `Into<Var>`.
/// Nestable.
///
/// ```
/// use mercury::{o, Var};
/// let user = o!{
///     "name": "bob",
///     "age": 30i64,
///     "tags": mercury::a!["admin", "dev"],
///     "stats": o!{ "score": 9i64 },
/// };
/// assert_eq!(user.get("name").as_str(), Some("bob"));
/// ```
#[macro_export]
macro_rules! o {
    // base case: empty object
    () => { $crate::Object::new() };
    // one trailing "k": v
    ( $k:tt : $v:expr ) => {{
        let mut o = $crate::Object::new();
        o.set($k, $v.into());
        o
    }};
    // "k": v, rest...  (prepend head so source order is preserved)
    ( $k:tt : $v:expr, $($rest:tt)* ) => {{
        let mut tail = $crate::o!{ $($rest)* };
        tail.prepend($k, $v.into());
        tail
    }};
}

/// Build a list/array literal. Nestable.
///
/// ```
/// use mercury::a;
/// let xs = a![1i64, 2, 3];
/// assert_eq!(xs.len(), 3);
/// ```
#[macro_export]
macro_rules! a {
    () => { $crate::List::new() };
    ( $v:expr ) => {{
        let mut l = $crate::List::new();
        l.push($v.into());
        l
    }};
    // build forward: prepend head onto the recursively-built tail
    ( $v:expr, $($rest:tt)* ) => {{
        let mut tail = $crate::a!{ $($rest)* };
        tail.prepend($v.into());
        tail
    }};
}

/// Template-literal string builder. `$0`, `$1`, … are positional args (gold's
/// `tpl`). For inline interpolation prefer Rust's native `format!("...{x}")`.
///
/// ```
/// use mercury::tpl;
/// let s = tpl!("Hello $0, you scored $1", "bob", 9i64);
/// assert_eq!(s, "Hello bob, you scored 9");
/// ```
#[macro_export]
macro_rules! tpl {
    ( $fmt:expr ) => { $fmt.to_string() };
    ( $fmt:expr, $( $arg:expr ),+ $(,)? ) => {{
        let mut out = String::new();
        let vals: Vec<String> = vec![ $( ($arg).to_string()),+ ];
        let vals: &[String] = &vals;
        let mut chars = $fmt.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '$' {
                // collect the numeric index
                let mut num = String::new();
                while let Some(&d) = chars.peek() {
                    if d.is_ascii_digit() { num.push(d); chars.next(); } else { break; }
                }
                if let Ok(idx) = num.parse::<usize>() {
                    if idx < vals.len() { out.push_str(&vals[idx]); }
                    else if !num.is_empty() { out.push('$'); out.push_str(&num); }
                } else {
                    out.push('$');
                }
            } else {
                out.push(c);
            }
        }
        out
    }};
}

// ---- array helpers (JS-style) ----

/// Call `f` for each element (side effects).
pub fn each(list: &List, mut f: impl FnMut(&Var)) {
    for v in list.iter() { f(&v); }
}

/// Map each element to a new list.
pub fn map(list: &List, f: impl Fn(&Var) -> Var) -> List {
    let mut out = List::new();
    for v in list.iter() { out.push(f(&v)); }
    out
}

/// Keep elements where `f` is truthy.
pub fn filter(list: &List, f: impl Fn(&Var) -> bool) -> List {
    let mut out = List::new();
    for v in list.iter() { if f(&v) { out.push(v); } }
    out
}

/// First element where `f` is truthy, else `Null`.
pub fn find(list: &List, f: impl Fn(&Var) -> bool) -> Var {
    for v in list.iter() { if f(&v) { return v; } }
    Var::Null
}

/// Join elements as strings with `sep`.
pub fn join(list: &List, sep: &str) -> String {
    let mut out = String::new();
    let mut first = true;
    for v in list.iter() {
        if !first { out.push_str(sep); }
        out.push_str(&v.to_string());
        first = false;
    }
    out
}

/// Array literal convenience — `a![1,2,3]` already returns a `List`, but this
/// makes `Var`-typed arrays ergonomic: `arr![1, 2, 3]`.
#[macro_export]
macro_rules! arr {
    () => { $crate::List::new() };
    ( $($v:expr),* $(,)? ) => {{
        let mut l = $crate::List::new();
        $( l.push($v.into()); )*
        l
    }};
}
