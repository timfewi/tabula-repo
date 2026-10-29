use crate::storage::*;
use crate::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
fn registrations(root: &Path) -> Result<(V, V)> {
    let path = safe_path(root, ".tabula/cards.json")?;
    let mut cards = json!({"entry":".tabula/ENTRY.md"});
    if !path.exists() {
        return Ok((cards, V::Null));
    }
    let data = read_file(&path, 8000)?;
    let r = parse_json(&data)?;
    if !receipts::fields(&r, &["version", "cards"])
        || r["version"].as_u64() != Some(1)
        || !r["cards"].is_array()
    {
        return Err(Error::new(
            "malformed card registration; expected version 1 and a cards list",
        ));
    }
    if len(&r["cards"]) >= 16 {
        return Err(Error::new("too many registered cards").kind("oversized"));
    }
    let mut added = BTreeMap::new();
    for c in items(&r["cards"]) {
        if !receipts::fields(c, &["id", "path"])
            || !receipts::identity(s(&c["id"]))
            || !c["path"].is_string()
        {
            return Err(Error::new(
                "card registration needs a short lowercase id and relative Markdown path",
            ));
        }
        let relative = s(&c["path"]);
        r.path(relative)?;
        let path = safe_path(root, &Path::new(".tabula").join(relative).to_string_lossy())?;
        if Path::new(relative).is_absolute()
            || relative
                .split('/')
                .any(|x| x == ".." || x == "." || x.is_empty())
            || path.extension().is_none_or(|x| x != "md")
            || relative.split('/').count() > 4
        {
            return Err(Error::new(
                "card path must be a normalized .md path within .tabula and its depth limit",
            )
            .kind("unsafe"));
        }
        let name = storage::relative(root, &path);
        let id = s(&c["id"]);
        if id == "entry"
            || added.contains_key(id)
            || name == ".tabula/ENTRY.md"
            || added.values().any(|v| v == &name)
        {
            return Err(Error::new("duplicate card id or path"));
        }
        added.insert(id.to_string(), name);
    }
    for (k, v) in added {
        cards[&k] = json!(v);
    }
    Ok((cards, json!(digest(&data))))
}
fn url(target: &str) -> Result<(String, bool, bool)> {
    let target = target
        .trim_start_matches(|c: char| c as u32 <= 32)
        .replace(['\n', '\r', '\t'], "");
    let external = target.starts_with("//")
        || regex::Regex::new(r"^[A-Za-z][A-Za-z0-9+.-]*:")
            .unwrap()
            .is_match(&target);
    let path = target.split('#').next().unwrap_or("");
    let query = path.contains('?');
    let path = path.split('?').next().unwrap_or("");
    if external || path.is_empty() {
        return Ok((String::new(), external, true));
    }
    if query || path.starts_with('/') {
        return Err(
            Error::new("source links must be relative paths without a query").kind("unsafe"),
        );
    }
    let mut bytes = Vec::new();
    let mut i = 0;
    let b = path.as_bytes();
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Some(n) = (b[i + 1] as char)
                .to_digit(16)
                .zip((b[i + 2] as char).to_digit(16))
                .map(|(hi, lo)| (hi * 16 + lo) as u8)
            {
                bytes.push(n);
                i += 3;
                continue;
            }
        }
        bytes.push(b[i]);
        i += 1;
    }
    let decoded = String::from_utf8(bytes)
        .map_err(|_| Error::new("Unreadable or malformed input; inspect the selected file."))?;
    Ok((decoded, false, false))
}
struct Inspection<'a> {
    root: &'a Path,
    paths: V,
    sources: V,
    errors: BTreeMap<String, Error>,
}
impl Inspection<'_> {
    fn card(&mut self, name: &str, data: Option<&[u8]>, identity: &str, remaining: usize) -> V {
        let mut report = json!({"path":name,"bytes":null,"limit":10000,"remaining_bytes":null,"exceeded_bytes":null,"sections":{},"navigation":[],"topics":[],"diagnostics":[]});
        let parsed = (|| -> Result<V> {
            let path = safe_path(self.root, name)?;
            let size = if let Some(d) = data {
                d.len()
            } else {
                std::fs::metadata(&path)?.len() as usize
            };
            report["bytes"] = json!(size);
            report["remaining_bytes"] = json!(10000usize.saturating_sub(size));
            report["exceeded_bytes"] = json!(size.saturating_sub(10000));
            let read;
            let data = if let Some(d) = data {
                d
            } else {
                read = read_file(&path, 10000.min(remaining))?;
                &read
            };
            if data.len() > 10000 {
                return Err(Error::new("card exceeds its byte budget").kind("oversized"));
            }
            report["sha256"] = json!(digest(data));
            let text = std::str::from_utf8(data).map_err(|_| {
                Error::new("Unreadable or malformed input; inspect the selected file.")
            })?;
            if text
                .chars()
                .any(|c| (c < ' ' && !matches!(c, '\r' | '\n' | '\t')) || c == '\u{7f}')
            {
                return Err(
                    Error::new("control characters are not allowed in cards").kind("unsafe")
                );
            }
            markdown::markdown(text)
        })();
        let parsed = match parsed {
            Ok(v) => v,
            Err(e) => {
                report["diagnostics"]
                    .as_array_mut()
                    .unwrap()
                    .push(e.diagnostic(Some(name), None));
                return report;
            }
        };
        let mut card_sources = obj();
        for (section_id, section) in entries(&parsed) {
            let mut section = section.clone();
            let mut deps = obj();
            for link in items(&section["links"]) {
                let line = link[0].as_u64().unwrap() as usize;
                let target = s(&link[1]);
                let mut source = None;
                let result = (|| -> Result<()> {
                    let (path, external, navigation) = url(target)?;
                    if navigation {
                        report["navigation"].as_array_mut().unwrap().push(
                            json!({"line":line,"kind":if external{"external"}else{"anchor"}}),
                        );
                        return Ok(());
                    }
                    let path = safe_path(
                        self.root,
                        &Path::new(name)
                            .parent()
                            .unwrap()
                            .join(path)
                            .to_string_lossy(),
                    )?;
                    if path.is_dir() {
                        report["navigation"]
                            .as_array_mut()
                            .unwrap()
                            .push(json!({"line":line,"kind":"directory"}));
                        return Ok(());
                    }
                    let src = relative(self.root, &path);
                    source = Some(src.clone());
                    if let Some(id) = self.paths.get(&src) {
                        let topic = json!({"line":line,"kind":"topic","id":id,"path":src,"section":format!("{identity}/{section_id}")});
                        report["navigation"]
                            .as_array_mut()
                            .unwrap()
                            .push(topic.clone());
                        report["topics"].as_array_mut().unwrap().push(topic);
                        return Ok(());
                    }
                    if src.starts_with(".tabula/") {
                        return Err(Error::new("unregistered card or generated state; register Markdown cards in cards.json").kind("unsafe"));
                    }
                    if self.sources.get(&src).is_none() {
                        if len(&self.sources) >= 64 {
                            return Err(Error::new("too many cited source files").kind("oversized"));
                        }
                        self.sources[&src] = V::Null;
                        match read_file(&path, 2 * 1024 * 1024) {
                            Ok(data) => self.sources[&src] = json!(digest(&data)),
                            Err(e) => {
                                self.errors.insert(src.clone(), e);
                            }
                        }
                    }
                    if let Some(e) = self.errors.get(&src) {
                        return Err(e.clone());
                    }
                    deps[&src] = self.sources[&src].clone();
                    Ok(())
                })();
                if let Err(e) = result {
                    report["diagnostics"]
                        .as_array_mut()
                        .unwrap()
                        .push(e.diagnostic(Some(name), Some(line)));
                    if let Some(src) = source {
                        if let Some(v) = self.sources.get(&src) {
                            deps[&src] = v.clone();
                        }
                    }
                }
            }
            for (k, v) in entries(&deps) {
                card_sources[k] = v.clone();
            }
            let fingerprint = digest(&compat::canonical(
                &json!({"text":section["text"],"ancestors":section["ancestors"],"links":items(&section["links"]).map(|l|l[1].clone()).collect::<Vec<_>>(),"sources":deps}),
            ));
            section["sources"] = deps;
            section["fingerprint"] = json!(fingerprint);
            report["sections"][format!("{identity}/{section_id}")] = section;
        }
        if len(&card_sources) == 0 {
            report["diagnostics"].as_array_mut().unwrap().push(
                Error::new("card needs at least one relative source-file link")
                    .diagnostic(Some(name), None),
            );
        }
        report["sources"] = card_sources;
        report
    }
}
pub fn inspect_repository(root: &Path, entry: Option<&[u8]>) -> V {
    let mut report = json!({"cards":{},"sources":{},"diagnostics":[],"registration_sha256":null});
    let cards = match registrations(root) {
        Ok((cards, hash)) => {
            report["registration_sha256"] = hash;
            cards
        }
        Err(e) => {
            report["diagnostics"]
                .as_array_mut()
                .unwrap()
                .push(e.diagnostic(Some(".tabula/cards.json"), None));
            json!({"entry":".tabula/ENTRY.md"})
        }
    };
    let mut paths = obj();
    for (id, name) in entries(&cards) {
        paths[s(name)] = json!(id);
    }
    let mut inspection = Inspection {
        root,
        paths,
        sources: obj(),
        errors: BTreeMap::new(),
    };
    let mut remaining = 80000;
    for (id, name) in entries(&cards) {
        let card = inspection.card(
            s(name),
            if id == "entry" { entry } else { None },
            id,
            remaining,
        );
        remaining = remaining.saturating_sub(card["bytes"].as_u64().unwrap_or(0) as usize);
        report["diagnostics"]
            .as_array_mut()
            .unwrap()
            .extend(items(&card["diagnostics"]).cloned());
        report["cards"][id] = card;
    }
    report["sources"] = inspection.sources;
    fn visit(
        id: &str,
        cards: &V,
        trail: &mut Vec<String>,
        depths: &mut BTreeMap<String, usize>,
    ) -> Result<usize> {
        if trail.iter().any(|s| s == id) {
            return Err(Error::new("card-link cycle; remove a topic back-link"));
        }
        if let Some(d) = depths.get(id) {
            return Ok(*d);
        }
        trail.push(id.into());
        let children: BTreeSet<_> = items(&cards[id]["topics"]).map(|t| s(&t["id"])).collect();
        let mut depth = 1;
        for child in children {
            depth = depth.max(1 + visit(child, cards, trail, depths)?);
        }
        trail.pop();
        if depth > 4 {
            return Err(
                Error::new("card-link depth exceeds the configured limit").kind("oversized")
            );
        }
        depths.insert(id.into(), depth);
        Ok(depth)
    }
    let mut depths = BTreeMap::new();
    for (id, _) in entries(&cards) {
        if let Err(e) = visit(id, &report["cards"], &mut vec![], &mut depths) {
            report["diagnostics"]
                .as_array_mut()
                .unwrap()
                .push(e.diagnostic(Some(".tabula/cards.json"), None));
            break;
        }
    }
    report
}
pub fn snapshot(root: &Path, entry: Option<&[u8]>) -> Result<V> {
    let report = inspect_repository(root, entry);
    if let Some(d) = items(&report["diagnostics"]).next() {
        if d["kind"] == "missing" {
            return Err(Error::from(std::io::Error::from(
                std::io::ErrorKind::NotFound,
            )));
        }
        return Err(Error {
            kind: s(&d["kind"]).into(),
            message: s(&d["message"]).into(),
            file: d["file"].as_str().map(String::from),
            line: d["line"].as_u64().map(|n| n as usize),
        });
    }
    let mut cards = obj();
    for (id, c) in entries(&report["cards"]) {
        let mut sections = obj();
        for (key, sec) in entries(&c["sections"]) {
            sections[key] = json!({"fingerprint":sec["fingerprint"],"sources":sec["sources"]});
        }
        cards[id] = json!({"path":c["path"],"sha256":c["sha256"],"sections":sections});
    }
    let mut sources = obj();
    let sorted: BTreeMap<_, _> = entries(&report["sources"]).collect();
    for (k, v) in sorted {
        sources[k] = v.clone();
    }
    Ok(
        json!({"version":3,"entry_sha256":cards["entry"]["sha256"],"sources":sources,"cards":cards,"reviews":{},"registration_sha256":report["registration_sha256"],"tests":[]}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeated_evidence_is_read_once() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("README.md"), "source").unwrap();
        std::fs::create_dir(temp.path().join(".tabula")).unwrap();
        std::fs::write(
            temp.path().join(".tabula/ENTRY.md"),
            "# Fixture\n[a](../README.md) [b](../README.md)\n## Detail\n[c](../README.md)\n",
        )
        .unwrap();
        storage::SOURCE_READS.with(|n| n.set(0));
        let report = inspect_repository(temp.path(), None);
        assert_eq!(len(&report["diagnostics"]), 0);
        assert_eq!(storage::SOURCE_READS.with(|n| n.get()), 1);
    }
}
