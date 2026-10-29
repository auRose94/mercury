//! `List` — an ordered dynamic array of `Var`, shared via `Rc<RefCell<…>>`.
//!
//! gold's `list` is a `vector<var>` behind a shared_ptr with per-op locking.
//! We keep the ordered-array shape, and use `Rc<RefCell<Vec>>` for the
//! payload: cheap copy-by-pointer (COW-ish, but actually shared-mutable) and
//! interior mutability, which is what lets the `VarRef` proxy mutate the
//! original list in place through one handle. (gold locks instead; we rely on
//! the borrow checker for single-threaded safety and can swap in `Rc<Mutex>`
//! for the multithreaded case.)

use std::fmt;
use std::cell::RefCell;
use std::rc::Rc;

use crate::var::Var;

#[derive(Clone, Default)]
pub struct List {
    items: Items,
}

/// Shared, interior-mutable ordered array.
type Items = Rc<RefCell<Vec<Var>>>;

impl std::fmt::Debug for List {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

impl PartialEq for List {
    fn eq(&self, other: &Self) -> bool {
        let a = self.items.borrow();
        let b = other.items.borrow();
        a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x == y)
    }
}

impl List {
    pub fn new() -> Self { List::default() }

    pub fn len(&self) -> usize { self.items.borrow().len() }

    pub fn is_empty(&self) -> bool { self.items.borrow().is_empty() }

    /// Read index `i`. Panics out of range (could return `Option` later).
    pub fn get(&self, i: usize) -> Var {
        self.items.borrow()[i].clone()
    }

    /// Alias for `get` matching the `Var`-style `get_idx` naming.
    pub fn get_idx(&self, i: usize) -> Var { self.get(i) }

    /// Snapshot iterator (clones each element). Borrowing past the `RefCell`
    /// isn't expressible, so this returns owned `Var`s.
    pub fn iter(&self) -> Vec<Var> {
        self.items.borrow().iter().cloned().collect()
    }

    /// Push an element. Shared payload is cloned first (RefCell borrow).
    pub fn push(&mut self, v: Var) {
        self.items.borrow_mut().push(v);
    }

    /// Set index `i` (used by the `VarRef` proxy). Grows with nulls if needed,
    /// JS-style (`arr[i] = x` extends the array). Takes `&self` — the payload
    /// is interior-mutable, so one handle can mutate and be seen by all.
    pub fn set(&self, i: usize, v: Var) {
        let mut items = self.items.borrow_mut();
        while items.len() <= i {
            items.push(Var::Null);
        }
        items[i] = v;
    }

    /// Insert an element at the *front* (used by the `a!` macro, which builds
    /// the tail recursively and prepends the head to preserve source order).
    pub fn prepend(&self, v: Var) {
        let mut items = self.items.borrow_mut();
        items.insert(0, v);
    }

    /// Append all elements of another list (used by the `a!` macro to build
    /// forward order). Takes `&self` — interior-mutable payload.
    pub fn extend(&self, other: &List) {
        let mut items = self.items.borrow_mut();
        for v in other.items.borrow().iter() {
            items.push(v.clone());
        }
    }
}

impl fmt::Display for List {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[")?;
        for (i, v) in self.items.borrow().iter().enumerate() {
            if i > 0 { write!(f, ", ")?; }
            write!(f, "{v}")?;
        }
        write!(f, "]")
    }
}
