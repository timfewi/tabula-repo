use crate::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
pub fn render(result: &V, as_json: bool) -> String {
    if as_json {
        return compat::dumps(result, false, false, false, false) + "\n";
    }
    if result.get("selection").is_none() {
        return entries(result)
            .map(|(k, v)| format!("{k}: {}", compat::pystr(v)))
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
    }
    let mut lines = vec![
        "Tabula context (UTF-8 bytes)".into(),
        "Caller assertions are not proof of correctness or test execution.".into(),
    ];
    for key in [
        "status",
        "selection",
        "unknown_paths",
        "unrelated_sections",
        "omitted",
        "related_topics",
        "omitted_topics",
        "tests",
    ] {
        lines.push(format!(
            "{key}: {}",
            compat::dumps(&result[key], false, false, true, false)
        ));
    }
    for section in items(&result["sections"]) {
        let mut metadata = section.clone();
        metadata.as_object_mut().unwrap().shift_remove("text");
        lines.extend([
            String::new(),
            compat::dumps(&metadata, false, false, true, false),
            s(&section["text"]).into(),
        ]);
    }
    lines.join("\n") + "\n"
}
pub fn context(root: &Path, names: &[String], max_bytes: i64, as_json: bool) -> Result<V> {
    if max_bytes <= 0 {
        return Err(Error::new(
            "--max-bytes must be a positive UTF-8 byte budget",
        ));
    }
    let selected = storage::requested_files(root, names)?;
    let report = cards::inspect_repository(root, None);
    if len(&report["diagnostics"]) > 0 {
        return Ok(json!({"status":"error","diagnostics":report["diagnostics"]}));
    }
    let stored = receipts::load_receipt(root, false)?;
    let comparison = operations::compare(&report, &stored);
    let all = receipts::state_sections(&report);
    let direct = entries(&all)
        .filter(|(_, section)| {
            entries(&section["sources"]).any(|(name, _)| selected.contains(name))
        })
        .map(|(k, _)| k.clone())
        .collect::<Vec<_>>();
    let mut candidates = direct
        .iter()
        .map(|k| (k.clone(), "exact source dependency"))
        .collect::<Vec<_>>();
    if !direct.is_empty() {
        let mut selected_cards: BTreeSet<_> = direct
            .iter()
            .map(|k| k.split('/').next().unwrap())
            .collect();
        selected_cards.insert("entry");
        for (id, card) in entries(&report["cards"]) {
            if selected_cards.contains(id.as_str()) {
                if let Some((key, _)) = entries(&card["sections"]).next() {
                    if !direct.contains(key) {
                        candidates.push((key.clone(), "card orientation"));
                    }
                }
            }
        }
    }
    let direct_cards: BTreeSet<_> = direct
        .iter()
        .map(|k| k.split('/').next().unwrap())
        .collect();
    let mut related = BTreeMap::new();
    for (id, card) in entries(&report["cards"]) {
        if id == "entry" || direct_cards.contains(id.as_str()) {
            for topic in items(&card["topics"]) {
                let id = s(&topic["id"]);
                if direct_cards.contains(id) {
                    related.entry(id.to_string()).or_insert(topic.clone());
                }
            }
        }
    }
    let mut result = json!({"status":if direct.is_empty(){"no_matches"}else{"ok"},"budget_unit":"UTF-8 bytes","selection":"exact dependencies first, then card orientation and related topic links","unknown_paths":selected.iter().filter(|k|report["sources"].get(*k).is_none()).collect::<Vec<_>>(),"unrelated_sections":len(&all)-candidates.len(),"sections":[],"omitted":candidates.iter().map(|(k,r)|json!({"id":k,"reason":format!("{r}: byte budget")})).collect::<Vec<_>>(),"related_topics":[],"omitted_topics":related.keys().collect::<Vec<_>>(),"tests":evidence::test_states(&stored["tests"],&report["sources"],Some(&selected))});
    if render(&result, as_json).len() > max_bytes as usize {
        return Err(Error::new(
            "byte budget cannot fit minimal context metadata; increase --max-bytes",
        ));
    }
    for (key, reason) in candidates {
        let id = key.split('/').next().unwrap();
        let card = &report["cards"][id];
        let section = &all[&key];
        let item = json!({"id":key,"card":card["path"],"line":section["line"],"reason":reason,"freshness":comparison["sections"][&key]["freshness"],"review":comparison["sections"][&key]["review"],"card_sha256":card["sha256"],"section_fingerprint":section["fingerprint"],"sources":section["sources"],"resolved_links":items(&section["links"]).map(|l|l[1].clone()).collect::<Vec<_>>(),"text":section["text"]});
        let omitted = result["omitted"].clone();
        result["omitted"]
            .as_array_mut()
            .unwrap()
            .retain(|x| x["id"] != key);
        result["sections"].as_array_mut().unwrap().push(item);
        if render(&result, as_json).len() > max_bytes as usize {
            result["sections"].as_array_mut().unwrap().pop();
            result["omitted"] = omitted;
        }
    }
    for (id, topic) in related {
        let omitted = result["omitted_topics"].clone();
        result["omitted_topics"]
            .as_array_mut()
            .unwrap()
            .retain(|x| *x != id);
        result["related_topics"].as_array_mut().unwrap().push(topic);
        if render(&result, as_json).len() > max_bytes as usize {
            result["related_topics"].as_array_mut().unwrap().pop();
            result["omitted_topics"] = omitted;
        }
    }
    Ok(result)
}
