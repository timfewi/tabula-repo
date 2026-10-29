use crate::*;
use crate::{receipts::*, storage::*};
use std::{collections::BTreeSet, path::Path};
pub fn test_states(records: &V, sources: &V, selected: Option<&BTreeSet<String>>) -> V {
    let mut results = vec![];
    for r in items(records) {
        if selected.is_some_and(|set| !entries(&r["sources"]).any(|(k, _)| set.contains(k))) {
            continue;
        }
        results.push(json!({"check_id":r["check_id"],"outcome":r["outcome"],"provenance":r["provenance"],"applicability":if entries(&r["sources"]).all(|(k,v)|sources.get(k)==Some(v)){"current"}else{"stale"},"sources":r["sources"]}));
    }
    json!({"status":if results.is_empty(){"missing"}else{"recorded"},"basis":"caller assertions, not proof of execution","results":results})
}
pub fn test_snapshot(root: &Path, names: &[String]) -> Result<V> {
    let current = cards::snapshot(root, None)?;
    let selected = requested_files(root, names)?;
    if !selected.iter().all(|k| current["sources"].get(k).is_some()) {
        return Err(Error::new(
            "test snapshot paths must be cited source files; inspect doctor",
        ));
    }
    let mut sources = obj();
    for name in selected {
        sources[&name] = current["sources"][&name].clone();
    }
    Ok(
        json!({"version":1,"check_id":"REPLACE_WITH_CHECK_ID","outcome":"unknown","provenance":"untrusted","sources":sources}),
    )
}
pub fn import_test(root: &Path, data: &[u8]) -> Result<V> {
    if data.len() > 16000 {
        return Err(Error::new("test evidence exceeds its byte budget").kind("oversized"));
    }
    let r = parse_json(data)?;
    validate_test(root, &r, &r.invalid_strings)?;
    let mut state = load_receipt(root, false)?;
    if state["version"] != 3 {
        return Err(Error::new(
            "use update --upgrade before importing test evidence",
        ));
    }
    let current = cards::snapshot(root, None)?;
    if !entries(&r["sources"]).all(|(k, _)| current["sources"].get(k).is_some()) {
        return Err(Error::new(
            "imported test inputs must be cited source files",
        ));
    }
    let mut records = items(&state["tests"])
        .filter(|x| x["check_id"] != r["check_id"])
        .cloned()
        .collect::<Vec<_>>();
    if records.len() >= 16 {
        return Err(
            Error::new("too many test results; replace an existing check identity")
                .kind("oversized"),
        );
    }
    records.push(r.value);
    records.sort_by(|a, b| s(&a["check_id"]).cmp(s(&b["check_id"])));
    state["tests"] = json!(records);
    write_receipt(root, &state)?;
    Ok(json!({"status":"imported","tests":test_states(&state["tests"],&current["sources"],None)}))
}
