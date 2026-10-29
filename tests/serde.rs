//! Serde round-trip tests: Var serializes to / from any serde format.
//! JSON is the representative one (via serde_json).

use mercury::{a, o, Null, Var};

#[test]
fn roundtrip_scalar() {
    for v in [
        Null,
        Var::from(true),
        Var::from(42i64),
        Var::from(3.5f64),
        Var::from("hello"),
    ] {
        let json = serde_json::to_string(&v).unwrap();
        let back: Var = serde_json::from_str(&json).unwrap();
        assert_eq!(back, v, "round-trip mismatch for {v}");
    }
}

#[test]
fn roundtrip_object_with_prototype() {
    // A nested object with a list and nested object.
    let v = o! {
        "name": "bob",
        "age": 30i64,
        "tags": a!["admin", "dev"],
        "stats": o!{ "score": 9i64 },
    };
    let json = serde_json::to_string(&v).unwrap();
    let back: Var = serde_json::from_str(&json).unwrap();
    assert_eq!(back, Var::from(v.clone()));
    // spot-check the structure survived
    assert_eq!(back.get("name").as_str(), Some("bob"));
    assert_eq!(back.get_path("stats.score").as_i64(), Some(9));
    assert_eq!(back.get("tags").get_idx(1).as_str(), Some("dev"));
}

#[test]
fn function_serializes_as_null() {
    // Functions have no data-model form; they serialize to null.
    let f = Var::Func(std::rc::Rc::new(|_| Var::from(1i64)));
    let json = serde_json::to_string(&f).unwrap();
    assert_eq!(json, "null");
}

#[test]
fn deserialize_into_typed_var() {
    // Deserialize straight into a typed Var (not just `Var`).
    let v: Var = serde_json::from_str(r#"{"a": [1, 2, 3], "b": true}"#).unwrap();
    assert_eq!(v.get("b").as_bool(), Some(true));
    assert_eq!(v.get("a").get_idx(2).as_i64(), Some(3));
}
