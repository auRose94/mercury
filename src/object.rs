//! `Object` — an ordered map of named `Var`, shared via `Rc<RefCell<…>>`, with
//! prototype inheritance (gold's `setParent`/`inherits`).
//!
//! Insertion order is preserved (gold's `object` is ordered), so we use
//! `indexmap::IndexMap` — the wheel for that, no hand-rolled ordered map. The
//! payload is `Rc<RefCell<IndexMap>>` so the `VarRef` proxy can mutate the
//! shared object in place. The prototype is `Rc<Object>` (an `Object` is
//! already internally shared).

use std::fmt;
use std::cell::RefCell;
use std::rc::Rc;

use indexmap::IndexMap;

use crate::var::{set_path_impl, Var};

#[derive(Clone, Default)]
pub struct Object {
    /// Own keys (insertion-ordered), interior-mutable so handles can mutate
    /// through one another.
    entries: OwnEntries,
    /// The prototype chain head. Lookups that miss own data walk this.
    parent: Option<Rc<Object>>,
}

/// Shared, interior-mutable ordered map of own keys.
type OwnEntries = Rc<RefCell<IndexMap<String, Var>>>;

impl std::fmt::Debug for Object {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_map().entries(self.entries.borrow().iter()).finish()
    }
}

impl PartialEq for Object {
    fn eq(&self, other: &Self) -> bool {
        let a = self.entries.borrow();
        let b = other.entries.borrow();
        a.len() == b.len()
            && a.iter()
                .all(|(k, v)| b.get(k).is_some_and(|bv| bv == v))
    }
}

impl Object {
    pub fn new() -> Self { Object::default() }

    /// True if `key` is declared in this object's own data (not the parent).
    pub fn owns(&self, key: &str) -> bool { self.entries.borrow().contains_key(key) }

    /// Own value for `key`, ignoring the prototype chain.
    pub fn get_own(&self, key: &str) -> Option<Var> {
        self.entries.borrow().get(key).cloned()
    }

    /// Value for `key`, walking the prototype chain on miss (gold's `getVar`).
    /// Returns `Null` on a miss — the JS `obj[k]` → `undefined` semantics.
    pub fn get(&self, key: &str) -> Var {
        if let Some(v) = self.entries.borrow().get(key) {
            return v.clone();
        }
        self.parent.as_ref().map(|p| p.get(key)).unwrap_or(Var::Null)
    }

    /// Proxy into `key` (gold's `obj["k"]`), for chained read/write.
    pub fn at(&self, key: &str) -> crate::proxy::VarRef {
        crate::proxy::VarRef::object(Var::from(self.clone()), key)
    }

    /// Read a dotted path: `o.get_path("stats.score")`.
    pub fn get_path(&self, path: &str) -> Var {
        let mut cur = Var::from(self.clone());
        for part in path.split('.') {
            cur = cur.get(part);
        }
        cur
    }

    /// Write a dotted path: `o.set_path("stats.score", 10)`, creating missing
    /// intermediate objects (JS `obj.a.b = 1` semantics).
    pub fn set_path(&self, path: &str, value: impl Into<Var>) {
        set_path_impl(&Var::from(self.clone()), path, value.into());
    }

    /// Set an own key. Mutates the shared map in place.
    pub fn set(&self, key: impl Into<String>, value: Var) {
        self.entries.borrow_mut().insert(key.into(), value);
    }

    /// Insert `key` as the *first* entry (used by the `o!` macro, which builds
    /// the tail recursively and prepends the head to preserve source order).
    pub fn prepend(&self, key: impl Into<String>, value: Var) {
        let key = key.into();
        let mut entries = self.entries.borrow_mut();
        let rest: Vec<(String, Var)> = entries
            .iter()
            .filter(|(k, _)| k.as_str() != key)
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        entries.clear();
        entries.insert(key, value);
        for (k, v) in rest {
            entries.insert(k, v);
        }
    }

    /// Copy all own entries from another object (used by the `o!` macro to
    /// build forward order). Later keys overwrite earlier ones.
    pub fn extend(&self, other: &Object) {
        let mut entries = self.entries.borrow_mut();
        for (k, v) in other.entries.borrow().iter() {
            entries.insert(k.clone(), v.clone());
        }
    }

    /// Remove an own key.
    pub fn remove(&self, key: &str) -> Option<Var> {
        self.entries.borrow_mut().shift_remove(key)
    }

    pub fn len(&self) -> usize { self.entries.borrow().len() }
    pub fn is_empty(&self) -> bool { self.entries.borrow().is_empty() }

    /// Snapshot of own entries (cloned). Borrowing past a `RefCell` borrow isn't
    /// expressible, so this returns owned copies — fine for display/debug/
    /// iteration; hot paths should use `get`/`get_own`.
    pub fn entries(&self) -> Vec<(String, Var)> {
        self.entries
            .borrow()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    /// Attach a prototype (gold's `setParent`).
    pub fn set_parent(&mut self, parent: Object) {
        self.parent = Some(Rc::new(parent));
    }

    /// Does `self` inherit (directly or transitively) from `other`? Identity is
    /// checked via the shared `entries` allocation, which uniquely tags an
    /// object instance.
    pub fn inherits(&self, other: &Object) -> bool {
        let other_ptr = Rc::as_ptr(&other.entries);
        let mut cur = &self.parent;
        while let Some(p) = cur {
            if Rc::as_ptr(&p.entries) == other_ptr { return true; }
            cur = &p.parent;
        }
        false
    }
}

impl fmt::Display for Object {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{{")?;
        for (i, (k, v)) in self.entries().iter().enumerate() {
            if i > 0 { write!(f, ", ")?; }
            write!(f, "{k}: {v}")?;
        }
        write!(f, "}}")
    }
}
