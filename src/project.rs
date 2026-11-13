//! Small, bounded additions to a project's local Tabula workflow.
use crate::storage::{read_file, safe_path, write_atomic, Error, Result};
use std::{fs, path::Path};

const LIMIT: usize = 1024 * 1024;
const GENERATED: [&str; 2] = ["/.tabula/state.json", "/.tabula/index.html"];

struct Change {
    name: &'static str,
    data: Vec<u8>,
    permissions: Option<fs::Permissions>,
}

fn read_optional(root: &Path, name: &str) -> Result<Option<(String, fs::Permissions)>> {
    let path = safe_path(root, name)?;
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if !metadata.is_file() {
        return Err(Error::new(format!("{name} must be a regular file")).kind("unsafe"));
    }
    let text = String::from_utf8(read_file(&path, LIMIT)?)
        .map_err(|_| Error::new(format!("{name} must be UTF-8 text")))?;
    Ok(Some((text, metadata.permissions())))
}

fn newline(text: &str) -> &'static str {
    if text
        .find('\n')
        .is_some_and(|index| index > 0 && text.as_bytes().get(index - 1) == Some(&b'\r'))
    {
        "\r\n"
    } else {
        "\n"
    }
}

fn append(text: &mut String, lines: &[&str]) {
    let eol = newline(text);
    if !text.is_empty() {
        if !text.ends_with('\n') {
            text.push_str(eol);
        }
        if !text.ends_with(&format!("{eol}{eol}")) {
            text.push_str(eol);
        }
    }
    for line in lines {
        text.push_str(line);
        text.push_str(eol);
    }
}

fn ignored_rules(text: &str) -> Vec<&'static str> {
    // An exact rule at the end takes precedence over earlier negations. Avoid
    // attempting to interpret the complete Git glob dialect: a later negation
    // conservatively requires a new exact rule, even when it targets another file.
    let lines: Vec<_> = text
        .trim_start_matches('\u{feff}')
        .lines()
        .map(|line| line.trim_end_matches([' ', '\t', '\r']))
        .collect();
    let negation = lines.iter().rposition(|line| line.starts_with('!'));
    GENERATED
        .into_iter()
        .filter(|rule| {
            lines
                .iter()
                .rposition(|line| line == rule)
                .is_none_or(|last| negation.is_some_and(|negation| negation > last))
        })
        .collect()
}

fn has_view_recipe(text: &str) -> bool {
    // Match declarations only, never indented recipe bodies. Parameters,
    // attributes on previous lines, quiet recipes and aliases retain their owner.
    let alias = regex::Regex::new(r"^alias[ \t]+tabula-view[ \t]*:=").unwrap();
    text.trim_start_matches('\u{feff}').lines().any(|line| {
        if alias.is_match(line) {
            return true;
        }
        let Some(tail) = line
            .strip_prefix("tabula-view")
            .or_else(|| line.strip_prefix("@tabula-view"))
        else {
            return false;
        };
        if !tail.starts_with([' ', '\t', ':']) {
            return false;
        }
        let tail = tail.trim_start_matches([' ', '\t']);
        // A variable may legally share a recipe's name. Its assignment operator
        // is not a recipe delimiter, even when the assigned value contains ':'.
        !tail.starts_with(":=") && !tail.starts_with('=') && tail.contains(':')
    })
}

/// Ignore generated local files and add a browser shortcut to an existing
/// Justfile. Existing recipes and all other file contents are preserved. All
/// selected paths and file budgets are checked before the first write.
pub fn prepare(root: &Path) -> Result<()> {
    let directory = safe_path(root, ".tabula")?;
    if directory.try_exists()? && !directory.is_dir() {
        return Err(Error::new(".tabula must be a directory").kind("unsafe"));
    }
    let output = safe_path(root, ".tabula/index.html")?;
    if output.try_exists()? && !output.is_file() {
        return Err(Error::new(".tabula/index.html must be a regular file").kind("unsafe"));
    }
    let mut changes = Vec::new();
    let existing = read_optional(root, ".gitignore")?;
    let (mut ignore, permissions) = existing
        .map(|(text, permissions)| (text, Some(permissions)))
        .unwrap_or_default();
    let missing = ignored_rules(&ignore);
    if !missing.is_empty() {
        let mut lines = vec!["# Generated Tabula files (local to this checkout)."];
        lines.extend(missing);
        append(&mut ignore, &lines);
        changes.push(Change {
            name: ".gitignore",
            data: ignore.into_bytes(),
            permissions,
        });
    }
    for name in ["Justfile", "justfile", ".justfile"] {
        let Some((mut text, permissions)) = read_optional(root, name)? else {
            continue;
        };
        if !has_view_recipe(&text) {
            append(
                &mut text,
                &[
                    "# Refresh and open the local Tabula overview.",
                    "tabula-view:",
                    "    @tabula view . --open",
                ],
            );
            changes.push(Change {
                name,
                data: text.into_bytes(),
                permissions: Some(permissions),
            });
        }
        break;
    }
    if changes.iter().any(|change| change.data.len() > LIMIT) {
        return Err(
            Error::new(format!("project integration exceeds {LIMIT} bytes")).kind("oversized"),
        );
    }
    for change in changes {
        write_atomic(root, change.name, &change.data)?;
        if let Some(permissions) = change.permissions {
            fs::set_permissions(safe_path(root, change.name)?, permissions)?;
        }
    }
    Ok(())
}
