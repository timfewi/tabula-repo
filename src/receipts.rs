use crate::storage::*;
use crate::*;
use std::path::Path;
pub fn fields(v: &V, keys: &[&str]) -> bool {
    v.as_object()
        .is_some_and(|m| m.len() == keys.len() && keys.iter().all(|k| m.contains_key(*k)))
}
pub fn identity(s: &str) -> bool {
    regex::Regex::new(r"\A[a-z][a-z0-9-]{0,31}\z")
        .unwrap()
        .is_match(s)
}
fn hash(v: &V) -> bool {
    v.as_str().is_some_and(|s| {
        s.len() == 64
            && s.bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    })
}
fn source_map(root: &Path, v: &V, invalid: &std::collections::BTreeSet<String>) -> Result<bool> {
    if !v.is_object() || len(v) > 64 {
        return Ok(false);
    }
    for (k, x) in entries(v) {
        if !hash(x) {
            return Ok(false);
        }
        crate::json::unicode_path(k, invalid)?;
        if relative(root, &safe_path(root, k)?) != *k || k.starts_with(".tabula/") {
            return Ok(false);
        }
    }
    Ok(true)
}
pub fn validate_test(
    root: &Path,
    r: &V,
    invalid: &std::collections::BTreeSet<String>,
) -> Result<()> {
    if !fields(
        r,
        &["version", "check_id", "outcome", "provenance", "sources"],
    ) {
        return Err(Error::new("test evidence needs exactly version, check_id, outcome, provenance and sources; logs are not accepted"));
    }
    if r["version"].as_u64() != Some(1)
        || !regex::Regex::new(r"\A[A-Za-z0-9][A-Za-z0-9._:/-]{0,79}\z")
            .unwrap()
            .is_match(s(&r["check_id"]))
    {
        return Err(Error::new(
            "test evidence needs version 1 and a short check identity",
        ));
    }
    if !["pass", "fail", "unknown"].contains(&s(&r["outcome"]))
        || !["caller", "untrusted"].contains(&s(&r["provenance"]))
        || len(&r["sources"]) == 0
        || !source_map(root, &r["sources"], invalid)?
    {
        return Err(Error::new(
            "test evidence needs explicit outcome, provenance and exact source hashes",
        ));
    }
    Ok(())
}
pub fn load_receipt(root: &Path, optional: bool) -> Result<V> {
    let path = safe_path(root, ".tabula/state.json")?;
    if optional && !path.exists() {
        return Ok(V::Null);
    }
    let parsed = parse_json(&read_file(&path, 512000)?)?;
    let invalid = &parsed.invalid_strings;
    let state = &parsed.value;
    if !state["version"].is_i64() && !state["version"].is_u64() {
        return Err(Error::new("malformed Tabula receipt"));
    }
    let version = state["version"].as_u64().unwrap_or(u64::MAX);
    if ![1, 2, 3].contains(&version) {
        return Err(Error::new(
            "unsupported receipt version; use a compatible Tabula release",
        ));
    }
    let mut keys = vec!["version", "entry_sha256", "sources"];
    if version >= 2 {
        keys.extend(["cards", "reviews"]);
    }
    if version == 3 {
        keys.extend(["registration_sha256", "tests"]);
    }
    if !fields(state, &keys) {
        return Err(Error::new("malformed receipt fields"));
    }
    if !hash(&state["entry_sha256"]) || !source_map(root, &state["sources"], invalid)? {
        return Err(Error::new("malformed Tabula receipt"));
    }
    if version == 1 {
        return Ok(state.clone());
    }
    let cards = &state["cards"];
    let reviews = &state["reviews"];
    if !cards.is_object() || cards.get("entry").is_none() || len(cards) > 16 || !reviews.is_object()
    {
        return Err(Error::new("malformed receipt cards or reviews"));
    }
    let mut sections = obj();
    for (id, c) in entries(cards) {
        if !fields(c, &["path", "sha256", "sections"])
            || !s(&c["path"]).starts_with(".tabula/")
            || !s(&c["path"]).ends_with(".md")
            || !hash(&c["sha256"])
            || !c["sections"].is_object()
            || len(&c["sections"]) > 256
        {
            return Err(Error::new("malformed receipt card"));
        }
        let path = s(&c["path"]);
        parsed.path(path)?;
        if relative(root, &safe_path(root, path)?) != path
            || Path::new(path).components().count() - 1 > 4
        {
            return Err(Error::new("malformed receipt card path"));
        }
        if !identity(id) {
            return Err(Error::new("malformed receipt card identity"));
        }
        for (key, section) in entries(&c["sections"]) {
            let valid = key.strip_prefix(&format!("{id}/")).is_some_and(|tail| {
                let Some((start, num)) = tail.rsplit_once('-') else {
                    return false;
                };
                let Some((level, title)) = start.split_once('-') else {
                    return false;
                };
                ["1", "2", "3", "4", "5", "6"].contains(&level)
                    && !title.is_empty()
                    && title.chars().all(|c| compat::is_word(c) || c == '-')
                    && !num.starts_with('0')
                    && !num.is_empty()
                    && num.bytes().all(|c| c.is_ascii_digit())
            });
            if !valid
                || !fields(section, &["fingerprint", "sources"])
                || !hash(&section["fingerprint"])
                || !source_map(root, &section["sources"], invalid)?
            {
                return Err(Error::new("malformed receipt section"));
            }
            sections[key] = section.clone();
        }
    }
    for (key, r) in entries(reviews) {
        if sections.get(key).is_none()
            || !fields(r, &["fingerprint", "card_sha256"])
            || !hash(&r["fingerprint"])
            || !hash(&r["card_sha256"])
        {
            return Err(Error::new("malformed review acknowledgement"));
        }
    }
    if cards["entry"]["path"] != ".tabula/ENTRY.md"
        || cards["entry"]["sha256"] != state["entry_sha256"]
    {
        return Err(Error::new("inconsistent entry receipt"));
    }
    let mut union = obj();
    for (_, section) in entries(&sections) {
        for (k, v) in entries(&section["sources"]) {
            if state["sources"].get(k) != Some(v) {
                return Err(Error::new("inconsistent source receipt"));
            }
            union[k] = v.clone();
        }
    }
    if union != state["sources"] {
        return Err(Error::new("inconsistent source inventory"));
    }
    if !state["registration_sha256"].is_null() && !hash(&state["registration_sha256"]) {
        return Err(Error::new("malformed registration receipt"));
    }
    if version == 3 {
        if !state["tests"].is_array() || len(&state["tests"]) > 16 {
            return Err(Error::new("malformed test evidence inventory"));
        }
        let mut ids = std::collections::BTreeSet::new();
        for r in items(&state["tests"]) {
            validate_test(root, r, invalid)?;
            if !ids.insert(s(&r["check_id"])) {
                return Err(Error::new("duplicate test identity"));
            }
        }
    }
    Ok(state.clone())
}
pub fn write_receipt(root: &Path, state: &V) -> Result<()> {
    let data = compat::dumps(state, true, true, true, false) + "\n";
    if data.len() > 512000 {
        return Err(Error::new("receipt exceeds its byte budget").kind("oversized"));
    }
    write_atomic(root, ".tabula/state.json", data.as_bytes())
}
pub fn state_sections(state: &V) -> V {
    let mut sections = obj();
    for (_, card) in entries(&state["cards"]) {
        for (k, v) in entries(&card["sections"]) {
            sections[k] = v.clone();
        }
    }
    sections
}
