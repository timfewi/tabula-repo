use crate::compat::{casefold, entity, is_space, is_word};
use crate::*;
use regex::Regex;
use std::collections::BTreeMap;
fn re(p: &str) -> Regex {
    Regex::new(p).unwrap()
}
fn st(t: &[char]) -> String {
    t.iter().collect()
}
fn blank(t: &[char]) -> Vec<char> {
    t.iter()
        .map(|c| if *c == '\n' { '\n' } else { ' ' })
        .collect()
}
fn unescape(s: &str) -> String {
    re(r##"\\([!"#$%&'()*+,\-./:;<=>?@\[\]\\^_`{|}~])"##)
        .replace_all(s, "$1")
        .into_owned()
}
fn label(s: &str) -> String {
    casefold(
        &unescape(s)
            .split(is_space)
            .filter(|x| !x.is_empty())
            .collect::<Vec<_>>()
            .join(" "),
    )
}
fn html(s: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '&' {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        let tail = st(&chars[i + 1..]);
        let mut consumed = 0;
        let mut value = String::new();
        if tail.starts_with('#') {
            let hex = tail.starts_with("#x") || tail.starts_with("#X");
            let start = if hex { 2 } else { 1 };
            let n = tail[start..]
                .chars()
                .take_while(|c| {
                    if hex {
                        c.is_ascii_hexdigit()
                    } else {
                        c.is_ascii_digit()
                    }
                })
                .count();
            if n > 0 {
                let code = u32::from_str_radix(&tail[start..start + n], if hex { 16 } else { 10 })
                    .unwrap_or(0x110000);
                consumed = start + n + usize::from(tail[start + n..].starts_with(';'));
                let code = match code {
                    0 => 0xfffd,
                    128 => 0x20ac,
                    130 => 0x201a,
                    131 => 0x192,
                    132 => 0x201e,
                    133 => 0x2026,
                    134 => 0x2020,
                    135 => 0x2021,
                    136 => 0x2c6,
                    137 => 0x2030,
                    138 => 0x160,
                    139 => 0x2039,
                    140 => 0x152,
                    142 => 0x17d,
                    145 => 0x2018,
                    146 => 0x2019,
                    147 => 0x201c,
                    148 => 0x201d,
                    149 => 0x2022,
                    150 => 0x2013,
                    151 => 0x2014,
                    152 => 0x2dc,
                    153 => 0x2122,
                    154 => 0x161,
                    155 => 0x203a,
                    156 => 0x153,
                    158 => 0x17e,
                    159 => 0x178,
                    x => x,
                };
                if !(matches!(code,1..=8|11|14..=31|127)
                    | ((0xfdd0..=0xfdef).contains(&code))
                    | (code & 0xffff == 0xfffe)
                    | (code & 0xffff == 0xffff))
                {
                    value.push(char::from_u32(code).unwrap_or('\u{fffd}'));
                }
            }
        } else {
            let end = tail
                .chars()
                .take(33)
                .take_while(|c| !matches!(c, '\t' | '\n' | '\u{c}' | ' ' | '<' | '&' | '#'))
                .count();
            for n in (1..=end).rev() {
                if let Some(v) = entity(&st(&chars[i + 1..i + 1 + n])) {
                    consumed = n;
                    value = v.into();
                    break;
                }
            }
        }
        if consumed == 0 {
            out.push('&');
            i += 1;
        } else {
            out.push_str(&value);
            i += consumed + 1;
        }
    }
    out
}
fn code(t: &[char], start: usize) -> (usize, bool) {
    let mut end = start;
    while end < t.len() && t[end] == '`' {
        end += 1;
    }
    let tail = st(&t[end..]);
    let boundary = re(r"\r?\n[ \t]*\r?\n|\n {0,3}(?:#{1,6}\s|>|`{3}|~{3})")
        .find(&tail)
        .map(|m| end + tail[..m.start()].chars().count())
        .unwrap_or(t.len());
    let mut i = end;
    while i < boundary {
        if t[i] == '`' {
            let mut j = i;
            while j < boundary && t[j] == '`' {
                j += 1;
            }
            if j - i == end - start {
                return (j, true);
            }
            i = j;
        } else {
            i += 1;
        }
    }
    (end, false)
}
fn bracket(t: &[char], start: usize) -> Result<usize> {
    let mut depth = 1;
    let mut i = start + 1;
    while i < t.len() {
        match t[i] {
            '\\' => {
                i += 2;
                continue;
            }
            '`' => {
                i = code(t, i).0;
                continue;
            }
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    return Ok(i);
                }
                if t.get(i + 1) == Some(&'(') {
                    return Err(Error::new(
                        "nested links are unsupported; use separate source links",
                    ));
                }
            }
            _ => {}
        }
        i += 1;
    }
    Err(Error::new(
        "unclosed link label; escape literal brackets or use inline code",
    ))
}
fn destination(t: &[char], start: usize, inline: bool) -> Result<(String, usize)> {
    let mut i = start;
    while i < t.len() && is_space(t[i]) {
        i += 1;
    }
    let mut begin = i;
    let target;
    if t.get(i) == Some(&'<') {
        i += 1;
        begin = i;
        while i < t.len() && t[i] != '>' {
            if matches!(t[i], '\n' | '<') {
                return Err(Error::new(
                    "invalid angle destination; close it on the same line",
                ));
            }
            i += if t[i] == '\\' { 2 } else { 1 };
        }
        if i >= t.len() {
            return Err(Error::new("unclosed angle destination"));
        }
        target = st(&t[begin..i]);
        i += 1;
    } else {
        let mut depth = 0;
        while i < t.len() {
            let c = t[i];
            if c == '\\' {
                i += 2;
                continue;
            }
            if is_space(c) || (c == ')' && depth == 0) {
                break;
            }
            if c == '(' {
                depth += 1;
            } else if c == ')' {
                depth -= 1;
            }
            if matches!(c, '<' | '>') {
                return Err(Error::new(
                    "use angle brackets around destinations containing spaces",
                ));
            }
            i += 1;
        }
        if depth != 0 {
            return Err(Error::new(
                "unbalanced destination parentheses; escape literal delimiters",
            ));
        }
        target = st(&t[begin..i.min(t.len())]);
    }
    let after = i;
    while i < t.len() && is_space(t[i]) {
        i += 1;
    }
    if i > after && i < t.len() && matches!(t[i], '"' | '\'' | '(') {
        let close = if t[i] == '(' { ')' } else { t[i] };
        i += 1;
        while i < t.len() && t[i] != close {
            i += if t[i] == '\\' { 2 } else { 1 };
        }
        if i >= t.len() {
            return Err(Error::new("unclosed link title"));
        }
        i += 1;
        while i < t.len() && is_space(t[i]) {
            i += 1;
        }
    }
    if inline {
        if t.get(i) != Some(&')') {
            return Err(Error::new(
                "malformed inline link; use [label](<relative/path> \"title\")",
            ));
        }
        i += 1;
    } else if i != t.len() {
        return Err(Error::new(
            "malformed reference definition; use one destination and optional title",
        ));
    }
    Ok((html(&unescape(&target)), i))
}
fn lines(t: &[char]) -> Vec<Vec<char>> {
    let mut result = vec![];
    let mut start = 0;
    let mut i = 0;
    while i < t.len() {
        if matches!(
            t[i],
            '\n' | '\r'
                | '\u{b}'
                | '\u{c}'
                | '\u{1c}'
                | '\u{1d}'
                | '\u{1e}'
                | '\u{85}'
                | '\u{2028}'
                | '\u{2029}'
        ) {
            if t[i] == '\r' && t.get(i + 1) == Some(&'\n') {
                i += 1;
            }
            result.push(t[start..=i].to_vec());
            start = i + 1;
        }
        i += 1;
    }
    if start < t.len() {
        result.push(t[start..].to_vec());
    }
    result
}
pub fn markdown(text: &str) -> Result<V> {
    let t: Vec<char> = text.chars().collect();
    let ls = lines(&t);
    let mut masked = vec![];
    let mut fence: Option<(char, usize)> = None;
    let mut headings: Vec<(usize, usize, usize, String)> = vec![];
    let mut offset = 0;
    let mut list = false;
    let mut indented = false;
    for (n, line) in ls.iter().enumerate() {
        let number = n + 1;
        let line_s = st(line);
        let trimmed = line_s.trim_end_matches(['\r', '\n']);
        let marker_re = re(r"^ {0,3}(`{3,}|~{3,})(.*)$");
        let marker = marker_re.captures(trimmed);
        if let Some((ch, width)) = fence {
            masked.extend(blank(line));
            if let Some(m) = marker {
                if m[1].starts_with(ch)
                    && m[1].len() >= width
                    && m[2].trim_matches(is_space).is_empty()
                {
                    fence = None;
                }
            }
        } else if let Some(m) = marker {
            if m[1].starts_with('`') && m[2].contains('`') {
                return Err(Error::new("backtick fence info cannot contain backticks").line(number));
            }
            fence = Some((m[1].chars().next().unwrap(), m[1].len()));
            masked.extend(blank(line));
        } else if (line_s.starts_with("    ") || line_s.starts_with('\t'))
            && !line_s.trim_matches(is_space).is_empty()
        {
            if list {
                return Err(Error::new(
                    "indented list content is unsupported; use flat single-line items",
                )
                .line(number));
            }
            if !indented && n > 0 && !st(&ls[n - 1]).trim_matches(is_space).is_empty() {
                return Err(Error::new("indented paragraph continuation is unsupported; separate code with a blank line").line(number));
            }
            indented = true;
            masked.extend(blank(line));
        } else {
            if !line_s.trim_matches(is_space).is_empty() {
                indented = false;
                if list && (line_s.starts_with("  ") || line_s.starts_with('\t')) {
                    return Err(Error::new(
                        "nested list content is unsupported; use flat single-line items",
                    )
                    .line(number));
                }
                list = re(r"^ {0,3}(?:[-+*]|[0-9]+[.)]) +").is_match(&line_s);
            }
            if re(r"^ {0,3}(>|[-+*] +(?:>|`{3}|~{3}))").is_match(&line_s) {
                return Err(Error::new(
                    "container blocks are unsupported; move evidence to plain paragraphs or lists",
                )
                .line(number));
            }
            if let Some(h) =
                re(r"^ {0,3}(#{1,6})(?:[ \t]+|$)(.*?)(?:[ \t]+#+[ \t]*)?$").captures(trimmed)
            {
                headings.push((offset, number, h[1].len(), h[2].into()));
            } else if re(r"\A {0,3}(?:=+|-+)[ \t]*\r?\n?\z").is_match(&line_s)
                && n > 0
                && !st(&masked[offset - ls[n - 1].len()..])
                    .trim_matches(is_space)
                    .is_empty()
            {
                headings.push((
                    offset - ls[n - 1].len(),
                    number - 1,
                    if line_s.contains('=') { 1 } else { 2 },
                    st(&ls[n - 1]).trim_matches(is_space).into(),
                ));
            }
            masked.extend(line);
        }
        offset += line.len();
    }
    let mut visible: Vec<char> = masked;
    let mut chars = visible.clone();
    let mut destinations = BTreeMap::new();
    let mut i = 0;
    while i < visible.len() {
        if let Some(stop) = destinations.get(&i) {
            i = *stop;
            continue;
        }
        if visible[i] == '\\' {
            i += 2;
            continue;
        }
        if visible[i] == '[' {
            if let Ok(end) = bracket(&visible, i) {
                if visible.get(end + 1) == Some(&'(') {
                    if let Ok((_, stop)) = destination(&visible, end + 2, true) {
                        destinations.insert(end + 1, stop);
                    }
                } else if visible.get(end + 1) == Some(&':')
                    && st(&visible[visible[..i]
                        .iter()
                        .rposition(|c| *c == '\n')
                        .map(|x| x + 1)
                        .unwrap_or(0)..i])
                    .trim_matches(is_space)
                    .is_empty()
                {
                    let stop = visible[end..]
                        .iter()
                        .position(|c| *c == '\n')
                        .map(|n| n + end)
                        .unwrap_or(visible.len());
                    destinations.insert(end + 1, stop);
                }
            }
        }
        if visible[i] == '`' {
            let (stop, paired) = code(&visible, i);
            if paired {
                chars[i..stop].copy_from_slice(&blank(&visible[i..stop]));
            }
            i = stop;
        } else {
            i += 1;
        }
    }
    visible = chars;
    let mut html_visible = visible.clone();
    for (start, stop) in &destinations {
        html_visible[*start..*stop].copy_from_slice(&blank(&visible[*start..*stop]));
    }
    for (n, line) in lines(&html_visible).iter().enumerate() {
        if re(r"<\s*(?:/?[A-Za-z][A-Za-z0-9-]*(?:\s|/?>)|!--)").is_match(&st(line)) {
            return Err(Error::new(
                "HTML evidence is unsupported; use Markdown links or fenced examples",
            )
            .line(n + 1));
        }
    }
    headings.retain(|(start, _, _, _)| {
        !st(&visible[*start..])
            .split('\n')
            .next()
            .unwrap_or("")
            .trim_matches(is_space)
            .is_empty()
    });
    let mut definitions = BTreeMap::new();
    let mut chars = visible.clone();
    offset = 0;
    for (n, line) in lines(&visible).iter().enumerate() {
        let string = st(line);
        if let Some(m) = re(r"^ {0,3}\[").find(&string) {
            let start = m.end() - 1;
            let end = bracket(&visible, offset + start).map_err(|e| e.line(n + 1))? - offset;
            if end < line.len() && line.get(end + 1) == Some(&':') {
                let body: Vec<_> = st(&line[end + 2..])
                    .trim_matches(is_space)
                    .chars()
                    .collect();
                let (target, _) = destination(&body, 0, false).map_err(|e| e.line(n + 1))?;
                if target.is_empty() {
                    return Err(Error::new(
                        "reference definition needs a destination on the same line",
                    )
                    .line(n + 1));
                }
                let key = label(&st(&ls[n][start + 1..end]));
                if definitions.insert(key, target).is_some() {
                    return Err(
                        Error::new("duplicate reference definition; use a unique label")
                            .line(n + 1),
                    );
                }
                chars[offset..offset + line.len()].copy_from_slice(&blank(line));
            }
        }
        offset += line.len();
    }
    visible = chars;
    let mut links = vec![];
    i = 0;
    while i < visible.len() {
        let c = visible[i];
        if c == '\\' {
            i += 2;
            continue;
        }
        let line = visible[..i].iter().filter(|c| **c == '\n').count() + 1;
        if c == '!' && visible.get(i + 1) == Some(&'[') {
            return Err(
                Error::new("image evidence is unsupported; use an ordinary source link").line(line),
            );
        }
        if c == '<' {
            let tail = st(&visible[i..]);
            if let Some(a) =
                re(r"^<([A-Za-z][A-Za-z0-9+.-]*:[^<>\s]*|[^<>\s]+@[^<>\s]+)>").captures(&tail)
            {
                let target = if a[1].contains(':') {
                    a[1].to_string()
                } else {
                    format!("mailto:{}", &a[1])
                };
                links.push((i, line, target));
                i += a[0].chars().count();
                continue;
            }
        }
        if c != '[' {
            i += 1;
            continue;
        }
        let end = bracket(&visible, i).map_err(|e| e.line(line))?;
        let label_text = st(&t[i + 1..end]);
        let mut after = end + 1;
        let target;
        if visible.get(after) == Some(&'(') {
            let result = destination(&visible, after + 1, true).map_err(|e| e.line(line))?;
            target = result.0;
            after = result.1;
        } else {
            let key = if visible.get(after) == Some(&'[') {
                let ref_end = bracket(&visible, after).map_err(|e| e.line(line))?;
                let ref_label = st(&t[after + 1..ref_end]);
                after = ref_end + 1;
                label(if ref_label.is_empty() {
                    &label_text
                } else {
                    &ref_label
                })
            } else {
                label(&label_text)
            };
            target = definitions.get(&key).cloned().ok_or_else(|| {
                Error::new("unresolved reference; define [label]: path or escape literal brackets")
                    .line(line)
            })?;
        }
        links.push((i, line, target));
        i = after;
    }
    if headings.first().is_none_or(|h| h.0 != 0 || h.2 != 1) {
        return Err(Error::new("card must begin with a repository heading").line(1));
    }
    if headings.len() > 256 {
        return Err(Error::new("too many sections").kind("oversized"));
    }
    let mut result = obj();
    let mut occurrences = BTreeMap::new();
    let mut ancestors: Vec<(usize, String)> = vec![];
    for (pos, (start, line, level, title)) in headings.iter().enumerate() {
        let end = headings.get(pos + 1).map(|h| h.0).unwrap_or(t.len());
        let mut slug = String::new();
        let mut invalid = false;
        for c in casefold(title).chars() {
            if is_word(c) || c == '-' {
                slug.push(c);
                invalid = false;
            } else if !invalid {
                slug.push('-');
                invalid = true;
            }
        }
        let slug = slug.trim_matches('-');
        let key = format!("{level}-{}", if slug.is_empty() { "section" } else { slug });
        let count = occurrences.entry(key.clone()).or_insert(0);
        *count += 1;
        let id = format!("{key}-{count}");
        ancestors.retain(|(depth, _)| depth < level);
        result[&id] = json!({"line":line,"heading":title,"text":st(&t[*start..end]),"ancestors":ancestors.iter().map(|(_,h)|h).collect::<Vec<_>>(),"links":links.iter().filter(|(off,_,_)|start<=off&&*off<end).map(|(_,n,target)|json!([n,target])).collect::<Vec<_>>()});
        ancestors.push((*level, title.clone()));
    }
    Ok(result)
}
