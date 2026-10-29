use crate::*;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};
#[derive(Debug, Clone)]
pub struct Error {
    pub kind: String,
    pub message: String,
    pub file: Option<String>,
    pub line: Option<usize>,
}
pub type Result<T> = std::result::Result<T, Error>;
impl Error {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            kind: "malformed".into(),
            message: message.into(),
            file: None,
            line: None,
        }
    }
    pub fn kind(mut self, kind: &str) -> Self {
        self.kind = kind.into();
        self
    }
    pub fn line(mut self, line: usize) -> Self {
        self.line = Some(line);
        self
    }
    pub fn diagnostic(&self, file: Option<&str>, line: Option<usize>) -> V {
        json!({"kind":self.kind,"file":self.file.as_deref().or(file),"line":self.line.or(line),"message":self.message})
    }
}
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::new("Unreadable or malformed input; inspect the selected file.").kind(
            if e.kind() == std::io::ErrorKind::NotFound {
                "missing"
            } else {
                "unreadable"
            },
        )
    }
}
impl From<serde_json::Error> for Error {
    fn from(_: serde_json::Error) -> Self {
        Self::new("Unreadable or malformed input; inspect the selected file.")
    }
}
pub fn digest(data: &[u8]) -> String {
    Sha256::digest(data)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
pub fn safe_path(root: &Path, relative: &str) -> Result<PathBuf> {
    #[cfg(windows)]
    for component in relative.split(['/', '\\']) {
        let stem = component
            .split('.')
            .next()
            .unwrap_or("")
            .to_ascii_uppercase();
        if component.contains(':')
            || (!matches!(component, "." | "..") && component.ends_with(['.', ' ']))
            || ["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str())
            || ((stem.starts_with("COM") || stem.starts_with("LPT"))
                && stem.len() == 4
                && matches!(stem.as_bytes()[3], b'1'..=b'9'))
        {
            return Err(Error::new("Windows device names, alternate streams and ambiguous path components are not source evidence").kind("unsafe"));
        }
    }

    if relative.chars().any(|c| c < ' ' || c == '\u{7f}') {
        return Err(Error::new("control characters are not allowed in paths").kind("unsafe"));
    }
    let mut path = PathBuf::new();
    for c in root.join(relative).components() {
        match c {
            Component::ParentDir => {
                path.pop();
            }
            Component::CurDir => {}
            x => path.push(x.as_os_str()),
        }
    }
    let rel = path
        .strip_prefix(root)
        .map_err(|_| Error::new("path escapes the repository").kind("unsafe"))?;
    if rel.as_os_str().is_empty()
        || rel.components().any(|x| {
            let p = x.as_os_str().to_string_lossy();
            [
                ".git",
                ".ssh",
                ".gnupg",
                "node_modules",
                ".direnv",
                "__pycache__",
                ".env",
            ]
            .contains(&p.as_ref())
                || p.starts_with(".env.")
        })
    {
        return Err(Error::new("credential or runtime path is not source evidence").kind("unsafe"));
    }
    if path.extension().is_some_and(|x| {
        ["key", "pem", "db", "sqlite", "sqlite3", "log"]
            .contains(&x.to_string_lossy().to_lowercase().as_str())
    }) {
        return Err(Error::new("credential or runtime file is not source evidence").kind("unsafe"));
    }
    let mut current = root.to_path_buf();
    for part in rel.components() {
        current.push(part);
        if let Ok(m) = fs::symlink_metadata(&current) {
            if is_link(&m) {
                return Err(Error::new("symlink is not source evidence").kind("unsafe"));
            }
        }
    }
    Ok(path)
}
fn is_link(m: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        m.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        m.file_type().is_symlink()
    }
}
#[cfg(test)]
thread_local! { pub(crate) static SOURCE_READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }
pub fn read_file(path: &Path, limit: usize) -> Result<Vec<u8>> {
    #[cfg(test)]
    if limit == 2 * 1024 * 1024 {
        SOURCE_READS.with(|n| n.set(n.get() + 1));
    }

    #[cfg(unix)]
    let file: fs::File = rustix::fs::open(
        path,
        rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK,
        rustix::fs::Mode::empty(),
    )
    .map_err(std::io::Error::from)?
    .into();
    #[cfg(windows)]
    let file = {
        use std::os::windows::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .read(true)
            .custom_flags(0x00200000)
            .open(path)?
    };
    let m = file.metadata()?;
    if !m.is_file() || is_link(&m) {
        return Err(Error::new("expected a regular file").kind("unsafe"));
    }
    if m.len() > limit as u64 {
        return Err(Error::new(format!("file exceeds {limit} bytes")).kind("oversized"));
    }
    let mut data = Vec::new();
    file.take(limit as u64 + 1).read_to_end(&mut data)?;
    if data.len() > limit {
        return Err(Error::new("file exceeded its read budget").kind("oversized"));
    }
    Ok(data)
}
pub fn write_atomic(root: &Path, relative: &str, data: &[u8]) -> Result<()> {
    let path = safe_path(root, relative)?;
    let mut temp = tempfile::Builder::new()
        .prefix(".tabula-write-")
        .tempfile_in(path.parent().unwrap())?;
    temp.write_all(data)?;
    temp.flush()?;
    safe_path(root, relative)?;
    temp.persist(&path).map_err(|e| Error::from(e.error))?;
    Ok(())
}
pub fn requested_files(
    root: &Path,
    names: &[String],
) -> Result<std::collections::BTreeSet<String>> {
    if names.is_empty() || names.len() > 64 || names.iter().map(String::len).sum::<usize>() > 8000 {
        return Err(Error::new(
            "request between 1 and 64 repository-relative source paths within 8000 bytes",
        ));
    }
    names
        .iter()
        .map(|n| {
            if Path::new(n).is_absolute() {
                return Err(
                    Error::new("requested files must be repository-relative").kind("unsafe")
                );
            }
            Ok(relative(root, &safe_path(root, n)?))
        })
        .collect()
}
pub fn relative(root: &Path, path: &Path) -> String {
    let text = path.strip_prefix(root).unwrap().to_string_lossy();
    #[cfg(windows)]
    {
        text.replace('\\', "/")
    }
    #[cfg(not(windows))]
    {
        text.into_owned()
    }
}
pub fn parse_json(data: &[u8]) -> Result<crate::json::Parsed> {
    let decoded;
    let (width, little, skip) = if data.starts_with(&[0, 0, 254, 255]) {
        (4, false, 4)
    } else if data.starts_with(&[255, 254, 0, 0]) {
        (4, true, 4)
    } else if data.starts_with(&[254, 255]) {
        (2, false, 2)
    } else if data.starts_with(&[255, 254]) {
        (2, true, 2)
    } else if data.len() >= 4 && data[0] == 0 && data[1] == 0 {
        (4, false, 0)
    } else if data.len() >= 4 && data[2] == 0 && data[3] == 0 {
        (4, true, 0)
    } else if data.len() >= 2 && data[0] == 0 {
        (2, false, 0)
    } else if data.len() >= 2 && data[1] == 0 {
        (2, true, 0)
    } else {
        (1, false, 0)
    };
    let data = if width == 1 {
        data
    } else {
        let input = &data[skip..];
        let invalid = || Error::new("Unreadable or malformed input; inspect the selected file.");
        if input.len() % width != 0 {
            return Err(invalid());
        }
        decoded = if width == 2 {
            let units: Vec<u16> = input
                .chunks_exact(2)
                .map(|b| {
                    if little {
                        u16::from_le_bytes([b[0], b[1]])
                    } else {
                        u16::from_be_bytes([b[0], b[1]])
                    }
                })
                .collect();
            String::from_utf16(&units).map_err(|_| invalid())?
        } else {
            input
                .chunks_exact(4)
                .map(|b| {
                    let bytes = [b[0], b[1], b[2], b[3]];
                    char::from_u32(if little {
                        u32::from_le_bytes(bytes)
                    } else {
                        u32::from_be_bytes(bytes)
                    })
                    .ok_or_else(invalid)
                })
                .collect::<Result<String>>()?
        };
        decoded.as_bytes()
    };
    crate::json::parse(data)
}
