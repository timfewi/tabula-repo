//! JSON decoding for schema validation. No recursive parser stack.
//! Values below depth 32 are syntax-checked but represented as null: all accepted
//! Tabula schemas are shallower, so those subtrees can only belong to invalid input.
use crate::*;
use std::collections::BTreeSet;
pub struct Parsed {
    pub value: V,
    pub invalid_strings: BTreeSet<String>,
}
impl std::ops::Deref for Parsed {
    type Target = V;
    fn deref(&self) -> &V {
        &self.value
    }
}
impl Parsed {
    pub fn path(&self, s: &str) -> Result<()> {
        unicode_path(s, &self.invalid_strings)
    }
}
pub fn unicode_path(s: &str, invalid: &BTreeSet<String>) -> Result<()> {
    if invalid.contains(s) {
        Err(Error::new("paths must be valid UTF-8").kind("unsafe"))
    } else {
        Ok(())
    }
}
fn malformed() -> Error {
    Error::new("Unreadable or malformed input; inspect the selected file.")
}
#[derive(Clone)]
enum Token {
    Open(bool),
    Close(bool),
    Colon,
    Comma,
    Scalar(V),
    String(String),
}
fn string(data: &[u8], i: &mut usize) -> Result<Vec<u16>> {
    *i += 1;
    let mut units = vec![];
    while *i < data.len() {
        let c = data[*i];
        *i += 1;
        match c {
            b'"' => return Ok(units),
            b'\\' => {
                let escape = *data.get(*i).ok_or_else(malformed)?;
                *i += 1;
                match escape {
                    b'"' | b'\\' | b'/' => units.push(escape as u16),
                    b'b' => units.push(8),
                    b'f' => units.push(12),
                    b'n' => units.push(10),
                    b'r' => units.push(13),
                    b't' => units.push(9),
                    b'u' => {
                        let end = *i + 4;
                        let text = std::str::from_utf8(data.get(*i..end).ok_or_else(malformed)?)
                            .map_err(|_| malformed())?;
                        let n = u16::from_str_radix(text, 16).map_err(|_| malformed())?;
                        units.push(n);
                        *i = end;
                    }
                    _ => return Err(malformed()),
                }
            }
            0..=31 => return Err(malformed()),
            0x20..=0x7f => units.push(c as u16),
            _ => {
                *i -= 1;
                let tail = std::str::from_utf8(&data[*i..]).map_err(|_| malformed())?;
                let c = tail.chars().next().ok_or_else(malformed)?;
                *i += c.len_utf8();
                units.extend(c.encode_utf16(&mut [0; 2]).iter().copied());
            }
        }
    }
    Err(malformed())
}
enum Raw {
    Token(Token),
    String(Vec<u16>),
}
struct Frame {
    object: bool,
    values: V,
    key: Option<String>,
    keys: BTreeSet<String>,
    state: u8,
}
pub fn parse(data: &[u8]) -> Result<Parsed> {
    static INTEGER: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"^-?(?:0|[1-9][0-9]*)$").unwrap());
    static NUMBER: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"^-?(?:0|[1-9][0-9]*)(?:\.[0-9]+)?(?:[eE][+-]?[0-9]+)?$").unwrap()
    });
    let mut raw = vec![];
    let mut valid = BTreeSet::new();
    let mut i = if data.starts_with(&[0xef, 0xbb, 0xbf]) {
        3
    } else {
        0
    };
    while i < data.len() {
        let c = data[i];
        if matches!(c, b' ' | b'\t' | b'\n' | b'\r') {
            i += 1;
            continue;
        }
        if c == b'"' {
            let units = string(data, &mut i)?;
            if let Ok(s) = String::from_utf16(&units) {
                valid.insert(s);
            }
            raw.push(Raw::String(units));
            continue;
        }
        let token = match c {
            b'{' => Token::Open(true),
            b'[' => Token::Open(false),
            b'}' => Token::Close(true),
            b']' => Token::Close(false),
            b':' => Token::Colon,
            b',' => Token::Comma,
            _ => {
                let start = i;
                while i < data.len()
                    && !matches!(
                        data[i],
                        b' ' | b'\t' | b'\n' | b'\r' | b',' | b']' | b'}' | b':'
                    )
                {
                    i += 1;
                }
                if i == start {
                    return Err(malformed());
                }
                let word = &data[start..i];
                let text = std::str::from_utf8(word).map_err(|_| malformed())?;
                let v = if INTEGER.is_match(text) {
                    if text.trim_start_matches('-').len() > 4300 {
                        return Err(malformed());
                    }
                    // Only small integer versions occur in accepted schemas. Keep
                    // out-of-range integers distinguishable from floats and null.
                    if let Ok(n) = text.parse::<i64>() {
                        json!(n)
                    } else {
                        json!(text.parse::<u64>().unwrap_or(u64::MAX))
                    }
                } else if [b"NaN".as_slice(), b"Infinity", b"-Infinity"].contains(&word) {
                    json!("!nonfinite JSON number!")
                } else if let Ok(value) = serde_json::from_slice::<V>(word) {
                    value
                } else if NUMBER.is_match(text) {
                    json!("!nonfinite JSON number!")
                } else {
                    return Err(malformed());
                };
                raw.push(Raw::Token(Token::Scalar(v)));
                continue;
            }
        };
        i += 1;
        raw.push(Raw::Token(token));
    }
    let mut invalid = BTreeSet::new();
    let mut invalid_map: std::collections::BTreeMap<Vec<u16>, String> =
        std::collections::BTreeMap::new();
    let mut tokens = vec![];
    for r in raw {
        tokens.push(match r {
            Raw::Token(t) => t,
            Raw::String(units) => {
                let s = match String::from_utf16(&units) {
                    Ok(s) => s,
                    Err(_) => {
                        if let Some(s) = invalid_map.get(&units) {
                            s.clone()
                        } else {
                            let mut n = invalid.len();
                            let marker = loop {
                                let s = format!("!invalid-unicode-{n}!");
                                if !valid.contains(&s) && !invalid.contains(&s) {
                                    break s;
                                }
                                n += 1;
                            };
                            invalid.insert(marker.clone());
                            invalid_map.insert(units, marker.clone());
                            marker
                        }
                    }
                };
                Token::String(s)
            }
        });
    }
    let mut stack: Vec<Frame> = vec![];
    let mut root = None;
    fn attach(stack: &mut [Frame], root: &mut Option<V>, v: V) -> Result<()> {
        let depth = stack.len();
        if let Some(f) = stack.last_mut() {
            if f.object {
                if f.state != 2 {
                    return Err(malformed());
                }
                let key = f.key.take().ok_or_else(malformed)?;
                if depth <= 32 {
                    f.values[&key] = v;
                }
                f.state = 3;
            } else {
                if !matches!(f.state, 0 | 1) {
                    return Err(malformed());
                }
                if depth <= 32 {
                    f.values.as_array_mut().unwrap().push(v);
                }
                f.state = 3;
            }
        } else if root.replace(v).is_some() {
            return Err(malformed());
        }
        Ok(())
    }
    for token in tokens {
        match token {
            Token::Open(object) => {
                if let Some(f) = stack.last() {
                    if if f.object {
                        f.state != 2
                    } else {
                        !matches!(f.state, 0 | 1)
                    } {
                        return Err(malformed());
                    }
                } else if root.is_some() {
                    return Err(malformed());
                }
                stack.push(Frame {
                    object,
                    values: if object { obj() } else { json!([]) },
                    key: None,
                    keys: BTreeSet::new(),
                    state: 0,
                });
            }
            Token::Close(object) => {
                let f = stack.pop().ok_or_else(malformed)?;
                if f.object != object || !matches!(f.state, 0 | 3) {
                    return Err(malformed());
                }
                attach(&mut stack, &mut root, f.values)?;
            }
            Token::Colon => {
                let f = stack.last_mut().ok_or_else(malformed)?;
                if !f.object || f.state != 4 {
                    return Err(malformed());
                }
                f.state = 2;
            }
            Token::Comma => {
                let f = stack.last_mut().ok_or_else(malformed)?;
                if f.state != 3 {
                    return Err(malformed());
                }
                f.state = 1;
            }
            Token::String(s) => {
                if stack
                    .last()
                    .is_some_and(|f| f.object && matches!(f.state, 0 | 1))
                {
                    let f = stack.last_mut().unwrap();
                    if !f.keys.insert(s.clone()) {
                        return Err(Error::new("duplicate JSON field"));
                    }
                    f.key = Some(s);
                    f.state = 4;
                } else {
                    attach(&mut stack, &mut root, json!(s))?;
                }
            }
            Token::Scalar(v) => attach(&mut stack, &mut root, v)?,
        }
    }
    if !stack.is_empty() {
        return Err(malformed());
    }
    Ok(Parsed {
        value: root.ok_or_else(malformed)?,
        invalid_strings: invalid,
    })
}
