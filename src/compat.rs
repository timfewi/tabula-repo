use crate::*;
pub fn dumps(v: &V, ascii: bool, pretty: bool, spaces: bool, sort: bool) -> String {
    fn encode(v: &V, ascii: bool, pretty: bool, spaces: bool, sort: bool, level: usize) -> String {
        match v {
            V::String(s) => quote(s, ascii),
            V::Array(a) => {
                if a.is_empty() {
                    return "[]".into();
                }
                let sep = if pretty {
                    format!(",\n{}", "  ".repeat(level + 1))
                } else if spaces {
                    ", ".into()
                } else {
                    ",".into()
                };
                let body = a
                    .iter()
                    .map(|x| encode(x, ascii, pretty, spaces, sort, level + 1))
                    .collect::<Vec<_>>()
                    .join(&sep);
                if pretty {
                    format!(
                        "[\n{}{body}\n{}]",
                        "  ".repeat(level + 1),
                        "  ".repeat(level)
                    )
                } else {
                    format!("[{body}]")
                }
            }
            V::Object(m) => {
                if m.is_empty() {
                    return "{}".into();
                }
                let mut keys = m.keys().collect::<Vec<_>>();
                if sort {
                    keys.sort();
                }
                let sep = if pretty {
                    format!(",\n{}", "  ".repeat(level + 1))
                } else if spaces {
                    ", ".into()
                } else {
                    ",".into()
                };
                let body = keys
                    .iter()
                    .map(|k| {
                        format!(
                            "{}:{}{}",
                            quote(k, ascii),
                            if pretty || spaces { " " } else { "" },
                            encode(&m[*k], ascii, pretty, spaces, sort, level + 1)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(&sep);
                if pretty {
                    format!(
                        "{{\n{}{body}\n{}}}",
                        "  ".repeat(level + 1),
                        "  ".repeat(level)
                    )
                } else {
                    format!("{{{body}}}")
                }
            }
            _ => v.to_string(),
        }
    }
    encode(v, ascii, pretty, spaces, sort, 0)
}
pub fn quote(s: &str, ascii: bool) -> String {
    let raw = serde_json::to_string(s).unwrap();
    if !ascii {
        return raw;
    }
    let mut out = String::new();
    for c in raw.chars() {
        if c as u32 >= 127 {
            for u in c.encode_utf16(&mut [0; 2]) {
                out.push_str(&format!("\\u{u:04x}"));
            }
        } else {
            out.push(c);
        }
    }
    out
}
pub fn canonical(v: &V) -> Vec<u8> {
    dumps(v, true, false, false, true).into_bytes()
}
pub fn repr(v: &V) -> String {
    match v {
        V::Null => "None".into(),
        V::Bool(b) => if *b { "True" } else { "False" }.into(),
        V::String(t) => {
            let q = if t.contains('\'') && !t.contains('"') {
                '"'
            } else {
                '\''
            };
            let mut out = q.to_string();
            for c in t.chars() {
                match c {
                    '\n' => out.push_str("\\n"),
                    '\r' => out.push_str("\\r"),
                    '\t' => out.push_str("\\t"),
                    '\\' => out.push_str("\\\\"),
                    x if x == q => {
                        out.push('\\');
                        out.push(c);
                    }
                    x if !is_printable(x) => {
                        let n = x as u32;
                        out.push_str(&if n < 256 {
                            format!("\\x{n:02x}")
                        } else if n < 65536 {
                            format!("\\u{n:04x}")
                        } else {
                            format!("\\U{n:08x}")
                        });
                    }
                    _ => out.push(c),
                }
            }
            out.push(q);
            out
        }
        V::Array(a) => format!("[{}]", a.iter().map(repr).collect::<Vec<_>>().join(", ")),
        V::Object(m) => format!(
            "{{{}}}",
            m.iter()
                .map(|(k, v)| format!("{}: {}", repr(&json!(k)), repr(v)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        _ => v.to_string(),
    }
}
pub fn pystr(v: &V) -> String {
    if let V::String(t) = v {
        t.clone()
    } else {
        repr(v)
    }
}
pub use crate::unicode::{casefold, decimal, entity, is_printable, is_space, is_word};
