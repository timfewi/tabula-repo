use crate::*;
use crate::{evidence::test_states, receipts::*, storage::*};
use std::{collections::BTreeSet, io::Write, path::Path};
pub fn initialize(root: &Path, purpose: &str) -> Result<V> {
    if purpose.trim_matches(compat::is_space).is_empty() || purpose.contains(['\n', '\r']) {
        return Err(Error::new("purpose must be a nonempty single line"));
    }
    let dir = safe_path(root, ".tabula")?;
    let entry = safe_path(root, ".tabula/ENTRY.md")?;
    if entry.exists() || safe_path(root, ".tabula/state.json")?.exists() {
        return Err(Error::new(
            "Tabula already exists; edit the card and use update",
        ));
    }
    let mut paths = vec![];
    for name in [
        "README.md",
        "AGENTS.md",
        "flake.nix",
        ".project-checks.json",
        "pyproject.toml",
        "package.json",
        "Cargo.toml",
    ] {
        if safe_path(root, name)?.is_file() {
            paths.push(name);
        }
    }
    if paths.is_empty() {
        return Err(Error::new(
            "no repository declarations found; add a README first",
        ));
    }
    let mut text = format!(
        "# {}\n\n{}\n\n## Source entrypoints\n\n",
        root.file_name().unwrap_or_default().to_string_lossy(),
        purpose.trim_matches(compat::is_space)
    );
    text.push_str(
        &paths
            .iter()
            .map(|name| format!("- [{name}](../{name})"))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    text.push_str("\n\nRead the applicable instructions and declared checks before editing.\n");
    let state = cards::snapshot(root, Some(text.as_bytes()))?;
    if !dir.exists() {
        std::fs::create_dir(&dir)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&entry)?;
    file.write_all(text.as_bytes())?;
    drop(file);
    if let Err(e) = write_receipt(root, &state) {
        std::fs::remove_file(entry)?;
        return Err(e);
    }
    Ok(
        json!({"status":"created","sources":len(&state["sources"]),"review":"unreviewed; expand ownership and flow from source","tests":test_states(&json!([]),&obj(),None)}),
    )
}
pub fn update(root: &Path, upgrade: bool, acknowledge: &[String]) -> Result<V> {
    let previous = load_receipt(root, true)?;
    if !previous.is_null() && previous["version"] != 3 && !upgrade {
        return Err(Error::new(
            "older receipt: use update --upgrade for explicit migration",
        ));
    }
    let mut state = cards::snapshot(root, None)?;
    state["tests"] = previous.get("tests").cloned().unwrap_or(json!([]));
    let sections = state_sections(&state);
    for (key, review) in entries(&previous["reviews"]) {
        if sections
            .get(key)
            .is_some_and(|v| v["fingerprint"] == review["fingerprint"])
        {
            state["reviews"][key] = review.clone();
        }
    }
    for key in acknowledge {
        if sections.get(key).is_none() {
            return Err(Error::new(
                "unknown section; inspect check --json for section IDs",
            ));
        }
        let id = key.split('/').next().unwrap();
        state["reviews"][key] = json!({"fingerprint":sections[key]["fingerprint"],"card_sha256":state["cards"][id]["sha256"]});
    }
    write_receipt(root, &state)?;
    let mut review = obj();
    for (k, _) in entries(&sections) {
        review[k] = json!(if state["reviews"].get(k).is_some() {
            "acknowledged"
        } else {
            "unreviewed"
        });
    }
    Ok(
        json!({"status":"updated","sources":len(&state["sources"]),"review":review,"review_basis":"Caller assertion for exact section and source bytes; not proof of correctness.","tests":test_states(&state["tests"],&state["sources"],None)}),
    )
}
pub fn compare(report: &V, stored: &V) -> V {
    let names: BTreeSet<_> = entries(&report["sources"])
        .chain(entries(&stored["sources"]))
        .map(|(k, _)| k.clone())
        .collect();
    let mut changed: BTreeSet<String> = names
        .into_iter()
        .filter(|n| report["sources"].get(n) != stored["sources"].get(n))
        .collect();
    if report["registration_sha256"] != stored["registration_sha256"] {
        changed.insert(".tabula/cards.json".into());
    }
    let previous = state_sections(stored);
    let mut sections = obj();
    for (id, card) in entries(&report["cards"]) {
        let old_hash = if stored["cards"].get(id).is_some() {
            &stored["cards"][id]["sha256"]
        } else if id == "entry" && stored["version"] == 1 {
            &stored["entry_sha256"]
        } else {
            &V::Null
        };
        if card["sha256"] != *old_hash {
            changed.insert(s(&card["path"]).into());
        }
        for (key, section) in entries(&card["sections"]) {
            let mut stale = previous
                .get(key)
                .is_none_or(|old| old["fingerprint"] != section["fingerprint"]);
            if stored["version"] == 1 {
                stale = *old_hash != card["sha256"]
                    || entries(&section["sources"]).any(|(src, _)| changed.contains(src));
            }
            let review = stored["reviews"].get(key);
            let mut sources = entries(&section["sources"])
                .map(|(k, _)| k)
                .collect::<Vec<_>>();
            sources.sort();
            sections[key] = json!({"card":id,"line":section["line"],"heading":section["heading"],"freshness":if stale{"stale"}else{"current"},"review":match review{None=>"unreviewed",Some(r) if r["fingerprint"]==section["fingerprint"]&&entries(&section["sources"]).all(|(_,v)|!v.is_null())=>"acknowledged",_=>"invalidated"},"sources":sources});
        }
    }
    let removed: BTreeSet<_> = entries(&previous)
        .filter(|(k, _)| sections.get(*k).is_none())
        .map(|(k, _)| k)
        .collect();
    for k in removed {
        let mut sources = entries(&previous[k]["sources"])
            .map(|(k, _)| k)
            .collect::<Vec<_>>();
        sources.sort();
        sections[k] = json!({"card":k.split('/').next().unwrap(),"freshness":"removed","review":"invalidated","sources":sources});
    }
    for (id, c) in entries(&stored["cards"]) {
        if report["cards"].get(id).is_none() {
            changed.insert(s(&c["path"]).into());
        }
    }
    let affected = entries(&sections)
        .filter(|(_, v)| v["freshness"] != "current")
        .map(|(k, _)| k)
        .collect::<Vec<_>>();
    let mut cards = obj();
    for (k, c) in entries(&report["cards"]) {
        cards[k] = json!({"path":c["path"],"bytes":c["bytes"],"limit":c["limit"],"evidence_files":len(&c["sources"])});
    }
    json!({"status":if changed.is_empty(){"current"}else{"stale"},"changed":changed,"sources":len(&report["sources"]),"sections":sections,"affected_sections":affected,"receipt_version":stored["version"],"review_basis":"Caller assertion, separate from byte freshness and tests.","tests":test_states(&stored["tests"],&report["sources"],None),"cards":cards})
}
pub fn check(root: &Path) -> Result<V> {
    let report = cards::inspect_repository(root, None);
    let mut result = compare(&report, &load_receipt(root, false)?);
    if len(&report["diagnostics"]) > 0 {
        result["status"] = json!("error");
        result["diagnostics"] = report["diagnostics"].clone();
    }
    Ok(result)
}
pub fn doctor(root: &Path) -> Result<V> {
    let mut report = cards::inspect_repository(root, None);
    let receipt = match load_receipt(root, false) {
        Ok(v) => v,
        Err(e) => {
            report["diagnostics"]
                .as_array_mut()
                .unwrap()
                .push(e.diagnostic(Some(".tabula/state.json"), None));
            V::Null
        }
    };
    let comparison = if receipt.is_null() {
        V::Null
    } else {
        compare(&report, &receipt)
    };
    let mut cards = obj();
    for (id, c) in entries(&report["cards"]) {
        let mut value = obj();
        for (k, v) in entries(c) {
            if !["sections", "sources", "diagnostics"].contains(&k.as_str()) {
                value[k] = v.clone();
            }
        }
        value["evidence_files"] = json!(len(&c["sources"]));
        value["sections"] = json!(len(&c["sections"]));
        cards[id] = value;
    }
    Ok(
        json!({"status":if len(&report["diagnostics"])>0{"error"}else{"ok"},"limits":{"card_bytes":10000,"source_bytes":2097152,"source_files":64,"sections":256,"receipt_bytes":512000,"cards":16,"aggregate_card_bytes":80000,"card_depth":4,"registration_bytes":8000,"test_import_bytes":16000,"test_results":16},"cards":cards,"evidence_files":len(&report["sources"]),"diagnostics":report["diagnostics"],"affected_sections":comparison["affected_sections"],"sections":comparison.get("sections").cloned().unwrap_or(obj()),"aggregate_card_bytes":entries(&report["cards"]).map(|(_,c)|c["bytes"].as_u64().unwrap_or(0)).sum::<u64>(),"receipt_version":receipt["version"],"tests":test_states(&receipt["tests"],&report["sources"],None)}),
    )
}
