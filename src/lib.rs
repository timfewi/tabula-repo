#![forbid(unsafe_code)]
pub mod cards;
pub mod cli;
pub mod compat;
pub mod context;
pub mod evidence;
mod json;
pub mod markdown;
pub mod operations;
pub mod receipts;
pub mod storage;
pub(crate) use serde_json::{json, Map, Value as V};
pub(crate) use storage::{Error, Result};
pub(crate) fn obj() -> V {
    V::Object(Map::new())
}
pub(crate) fn entries(v: &V) -> impl Iterator<Item = (&String, &V)> {
    v.as_object().into_iter().flatten()
}
pub(crate) fn items(v: &V) -> impl Iterator<Item = &V> {
    v.as_array().into_iter().flatten()
}
pub(crate) fn s(v: &V) -> &str {
    v.as_str().unwrap_or("")
}
pub(crate) fn len(v: &V) -> usize {
    if let Some(m) = v.as_object() {
        m.len()
    } else if let Some(a) = v.as_array() {
        a.len()
    } else {
        0
    }
}

mod unicode;
