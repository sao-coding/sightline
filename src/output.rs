use crate::error::SightError;
use serde::Serialize;
use serde_json::{Value, json};

fn emit(v: &Value, pretty: bool) {
    let s = if pretty {
        serde_json::to_string_pretty(v)
    } else {
        serde_json::to_string(v)
    };
    println!("{}", s.expect("json serialization"));
}

pub fn success<T: Serialize>(data: &T, warnings: &[String], pretty: bool) {
    emit(
        &json!({"v": 1, "ok": true, "data": data, "warnings": warnings}),
        pretty,
    );
}

pub fn failure(e: &SightError, pretty: bool) {
    failure_with(e, None, &[], pretty);
}

/// Failure envelope that may still carry partial `data` (e.g. a saved image
/// after a settle timeout) and `warnings`.
pub fn failure_with(e: &SightError, data: Option<&Value>, warnings: &[String], pretty: bool) {
    let mut v = json!({"v": 1, "ok": false, "error": {
        "code": e.code(), "message": e.to_string(), "hint": e.hint()}});
    if let Some(d) = data {
        v["data"] = d.clone();
    }
    if !warnings.is_empty() {
        v["warnings"] = json!(warnings);
    }
    emit(&v, pretty);
}
