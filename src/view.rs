//! A local, derived HTML snapshot of the same evidence used by `check`.
use crate::storage::{safe_path, write_atomic, Result};
use crate::*;
use std::{path::Path, time::SystemTime};

pub const OUTPUT: &str = ".tabula/index.html";

fn escape(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '&' => "&amp;".into(),
            '<' => "&lt;".into(),
            '>' => "&gt;".into(),
            '"' => "&quot;".into(),
            '\'' => "&#39;".into(),
            c => c.to_string(),
        })
        .collect()
}

fn badge(value: &str) -> String {
    let tone = match value {
        "current" | "acknowledged" | "pass" => "good",
        "stale" | "invalidated" | "fail" | "removed" | "error" => "attention",
        _ => "neutral",
    };
    format!("<span class=\"badge {tone}\">{}</span>", escape(value))
}

fn path_list<'a>(paths: impl Iterator<Item = &'a str>) -> String {
    let paths: Vec<_> = paths
        .map(|path| format!("<li><code>{}</code></li>", escape(path)))
        .collect();
    if paths.is_empty() {
        "<span class=\"muted\">No cited sources</span>".into()
    } else {
        format!("<ul class=\"paths\">{}</ul>", paths.join(""))
    }
}

fn sections(report: &V, comparison: &V, has_receipt: bool) -> String {
    let mut rows = String::new();
    for (key, section) in entries(&comparison["sections"]) {
        let id = s(&section["card"]);
        let original = &report["cards"][id]["sections"][key];
        let heading = section["heading"]
            .as_str()
            .filter(|text| !text.is_empty())
            .unwrap_or(key);
        let freshness = if has_receipt {
            s(&section["freshness"])
        } else {
            "unrecorded"
        };
        let card_path = s(&report["cards"][id]["path"]);
        let line = section["line"]
            .as_u64()
            .map(|n| format!(":{n}"))
            .unwrap_or_default();
        let details = if s(&original["text"]).is_empty() {
            format!("<strong>{}</strong>", escape(heading))
        } else {
            format!(
                "<details><summary>{}</summary><pre class=\"prose\">{}</pre></details>",
                escape(heading),
                escape(s(&original["text"]))
            )
        };
        rows.push_str(&format!(
            "<tr data-section data-freshness=\"{}\"><th scope=\"row\">{}<span class=\"location\"><code>{}{}</code></span><span class=\"section-id\">{}</span></th><td>{}</td><td>{}</td><td>{}</td></tr>",
            escape(freshness), details, escape(card_path), line, escape(key),
            badge(freshness), badge(s(&section["review"])),
            path_list(items(&section["sources"]).map(s)),
        ));
    }
    if rows.is_empty() {
        rows.push_str("<tr><td colspan=\"4\" class=\"empty\">No readable sections. Inspect the diagnostics below and your .tabula/ENTRY.md.</td></tr>");
    }
    rows
}

fn cards(report: &V) -> String {
    let mut rows = String::new();
    for (id, card) in entries(&report["cards"]) {
        rows.push_str(&format!(
            "<tr><th scope=\"row\"><code>{}</code><span class=\"location\">{}</span></th><td>{}</td><td>{} / 10,000</td></tr>",
            escape(s(&card["path"])), escape(id), len(&card["sections"]),
            card["bytes"].as_u64().map(|n| n.to_string()).unwrap_or_else(|| "unreadable".into()),
        ));
    }
    rows
}

fn tests(comparison: &V) -> String {
    let mut rows = String::new();
    for test in items(&comparison["tests"]["results"]) {
        rows.push_str(&format!(
            "<tr><th scope=\"row\"><code>{}</code><span class=\"location\">{} assertion</span></th><td>{}</td><td>{}</td><td>{}</td></tr>",
            escape(s(&test["check_id"])), escape(s(&test["provenance"])),
            badge(s(&test["outcome"])), badge(s(&test["applicability"])),
            path_list(entries(&test["sources"]).map(|(name, _)| name.as_str())),
        ));
    }
    if rows.is_empty() {
        rows.push_str("<tr><td colspan=\"4\" class=\"empty\">No test evidence recorded. Tabula does not run project tests.</td></tr>");
    }
    rows
}

fn diagnostics(report: &V) -> String {
    let mut rows = String::new();
    for diagnostic in items(&report["diagnostics"]) {
        let line = diagnostic["line"]
            .as_u64()
            .map(|n| format!(":{n}"))
            .unwrap_or_default();
        rows.push_str(&format!(
            "<li><span class=\"diagnostic-kind\">{}</span><div><code>{}{}</code><p>{}</p></div></li>",
            escape(s(&diagnostic["kind"])), escape(s(&diagnostic["file"])), line,
            escape(s(&diagnostic["message"])),
        ));
    }
    if rows.is_empty() {
        "<p class=\"muted\">No input diagnostics.</p>".into()
    } else {
        format!("<ul class=\"diagnostics\">{rows}</ul>")
    }
}

/// Render current evidence without changing the stored baseline or review assertions.
pub fn write(root: &Path) -> Result<V> {
    safe_path(root, OUTPUT)?;
    let mut report = cards::inspect_repository(root, None);
    let stored = match receipts::load_receipt(root, true) {
        Ok(value) => value,
        Err(error) => {
            report["diagnostics"]
                .as_array_mut()
                .unwrap()
                .push(error.diagnostic(Some(".tabula/state.json"), None));
            V::Null
        }
    };
    let has_receipt = !stored.is_null();
    let comparison = operations::compare(&report, &stored);
    let state = if len(&report["diagnostics"]) > 0 {
        "error"
    } else if !has_receipt {
        "unrecorded"
    } else {
        s(&comparison["status"])
    };
    let state_message = match state {
        "current" => "The guide and cited source bytes match the saved receipt.",
        "stale" => "Source or guide bytes have changed. Review the affected explanations.",
        "unrecorded" => {
            "No saved receipt. Review the guide, then run tabula update . to record a baseline."
        }
        _ => "Some evidence could not be read or validated. Inspect the diagnostics below.",
    };
    let project = root.file_name().unwrap_or_default().to_string_lossy();
    let generated = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let section_count = len(&comparison["sections"]);
    let current_count = entries(&comparison["sections"])
        .filter(|(_, section)| has_receipt && section["freshness"] == "current")
        .count();
    let reviewed_count = entries(&comparison["sections"])
        .filter(|(_, section)| section["review"] == "acknowledged")
        .count();
    let test_count = len(&comparison["tests"]["results"]);
    let changed = if has_receipt && len(&comparison["changed"]) > 0 {
        path_list(items(&comparison["changed"]).map(s))
    } else if has_receipt {
        "<p class=\"muted\">No changed paths.</p>".into()
    } else {
        "<p class=\"muted\">A valid receipt is needed to compare paths.</p>".into()
    };
    let receipt = stored["version"]
        .as_u64()
        .map(|n| format!("Receipt v{n}"))
        .unwrap_or_else(|| "No valid receipt".into());
    let replacements = [
        ("<!--TABULA:PROJECT-->", escape(&project)),
        ("<!--TABULA:STATE-->", badge(state)),
        ("<!--TABULA:STATE-MESSAGE-->", state_message.into()),
        ("<!--TABULA:GENERATED-->", generated.to_string()),
        ("<!--TABULA:RECEIPT-->", receipt),
        ("<!--TABULA:SECTION-COUNT-->", section_count.to_string()),
        ("<!--TABULA:CURRENT-COUNT-->", current_count.to_string()),
        (
            "<!--TABULA:SOURCE-COUNT-->",
            len(&report["sources"]).to_string(),
        ),
        ("<!--TABULA:REVIEWED-COUNT-->", reviewed_count.to_string()),
        ("<!--TABULA:TEST-COUNT-->", test_count.to_string()),
        (
            "<!--TABULA:SECTIONS-->",
            sections(&report, &comparison, has_receipt),
        ),
        ("<!--TABULA:CARDS-->", cards(&report)),
        ("<!--TABULA:TESTS-->", tests(&comparison)),
        ("<!--TABULA:CHANGED-->", changed),
        ("<!--TABULA:DIAGNOSTICS-->", diagnostics(&report)),
    ];
    let mut html = include_str!("view.html").to_string();
    for (marker, value) in replacements {
        html = html.replace(marker, &value);
    }
    write_atomic(root, OUTPUT, html.as_bytes())?;
    Ok(
        json!({"status":"rendered","path":OUTPUT,"state":state,"sections":section_count,"sources":len(&report["sources"])}),
    )
}
