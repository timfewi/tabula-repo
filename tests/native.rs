#![forbid(unsafe_code)]
use serde_json::json;
use std::{fs, process::Command};
use tabula::{evidence, operations, storage};
#[test]
fn deeply_nested_and_duplicate_json_is_rejected_without_writes() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("README.md"), "source").unwrap();
    operations::initialize(root, "Fixture").unwrap();
    let before = fs::read(root.join(".tabula/state.json")).unwrap();
    for data in [
        format!("{}0{}", "[".repeat(2000), "]".repeat(2000)),
        "{\"version\":1,\"version\":1}".into(),
    ] {
        assert!(evidence::import_test(root, data.as_bytes()).is_err());
        assert_eq!(fs::read(root.join(".tabula/state.json")).unwrap(), before);
    }
}
#[test]
fn lone_surrogate_source_paths_are_unsafe() {
    let dir = tempfile::tempdir().unwrap();
    let raw = format!(
        r#"{{"version":1,"check_id":"test","outcome":"pass","provenance":"caller","sources":{{"\ud800":"{}"}}}}"#,
        "0".repeat(64)
    );
    let error = evidence::import_test(dir.path(), raw.as_bytes()).unwrap_err();
    assert_eq!(error.kind, "unsafe");
    assert_eq!(error.message, "paths must be valid UTF-8");
}
#[test]
fn native_binary_needs_no_interpreter_or_checkout() {
    let temp = tempfile::tempdir().unwrap();
    let binary = temp.path().join(if cfg!(windows) {
        "tabula.exe"
    } else {
        "tabula"
    });
    fs::copy(env!("CARGO_BIN_EXE_tabula"), &binary).unwrap();
    let p = Command::new(binary)
        .arg("--version")
        .env("PATH", "")
        .current_dir(temp.path())
        .output()
        .unwrap();
    assert!(p.status.success());
    assert_eq!(p.stdout, b"Tabula 0.1.0\n");
    assert!(p.stderr.is_empty());
}
#[test]
fn project_check_manifest_remains_input_without_execution() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(
        temp.path().join(".project-checks.json"),
        r#"{"checks":[{"argv":["touch","SHOULD_NOT_EXIST"]}]}"#,
    )
    .unwrap();
    let result = operations::initialize(temp.path(), "Fixture").unwrap();
    assert_eq!(result["sources"], json!(1));
    assert!(!temp.path().join("SHOULD_NOT_EXIST").exists());
}
#[test]
fn atomic_write_refuses_symlink() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::write(root.join("source"), "unchanged").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(root.join("source"), root.join("link")).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(root.join("source"), root.join("link")).unwrap();
    assert!(storage::write_atomic(root, "link", b"changed").is_err());
    assert_eq!(fs::read(root.join("source")).unwrap(), b"unchanged");
}
#[cfg(windows)]
#[test]
fn windows_locked_receipt_is_preserved() {
    use std::os::windows::fs::OpenOptionsExt;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::write(root.join("README.md"), "source").unwrap();
    operations::initialize(root, "Fixture").unwrap();
    let path = root.join(".tabula/state.json");
    let before = fs::read(&path).unwrap();
    let lock = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&path)
        .unwrap();
    assert!(operations::update(root, false, &[]).is_err());
    drop(lock);
    assert_eq!(fs::read(path).unwrap(), before);
}
#[cfg(windows)]
#[test]
fn windows_junction_is_not_source_evidence() {
    let temp = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("source"), "outside").unwrap();
    let link = temp.path().join("junction");
    let status = Command::new("cmd.exe")
        .args(["/d", "/c", "mklink", "/J"])
        .arg(&link)
        .arg(outside.path())
        .status()
        .unwrap();
    assert!(status.success());
    assert_eq!(
        storage::safe_path(temp.path(), "junction/source")
            .unwrap_err()
            .kind,
        "unsafe"
    );
    fs::remove_dir(link).unwrap();
}
#[test]
fn json_utf16_and_utf32_are_decoded() {
    let text = r#"{"version":1,"cards":[]}"#;
    let expected = serde_json::from_str::<serde_json::Value>(text).unwrap();
    let mut utf16 = vec![255, 254];
    for u in text.encode_utf16() {
        utf16.extend(u.to_le_bytes());
    }
    let mut utf32 = vec![0, 0, 254, 255];
    for c in text.chars() {
        utf32.extend((c as u32).to_be_bytes());
    }
    assert_eq!(storage::parse_json(&utf16).unwrap().value, expected);
    assert_eq!(storage::parse_json(&utf32).unwrap().value, expected);
}
#[test]
fn percent_followed_by_unicode_does_not_panic() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::write(root.join("%💡"), "source").unwrap();
    fs::create_dir(root.join(".tabula")).unwrap();
    fs::write(
        root.join(".tabula/ENTRY.md"),
        "# Fixture\n[source](../%💡)\n",
    )
    .unwrap();
    operations::update(root, false, &[]).unwrap();
    assert_eq!(operations::check(root).unwrap()["status"], "current");
}
#[cfg(windows)]
#[test]
fn windows_devices_and_alternate_streams_are_refused() {
    let temp = tempfile::tempdir().unwrap();
    for path in ["CON", "nul.txt", "source:stream", "file.", "file ", "COM1"] {
        assert_eq!(
            storage::safe_path(temp.path(), path).unwrap_err().kind,
            "unsafe"
        );
    }
}
#[test]
fn nonfinite_registration_hash_is_not_null() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::write(root.join("README.md"), "source").unwrap();
    operations::initialize(root, "Fixture").unwrap();
    let path = root.join(".tabula/state.json");
    let state = fs::read_to_string(&path).unwrap();
    fs::write(
        &path,
        state.replace(
            "\"registration_sha256\": null",
            "\"registration_sha256\": NaN",
        ),
    )
    .unwrap();
    assert_eq!(
        tabula::receipts::load_receipt(root, false)
            .unwrap_err()
            .message,
        "malformed registration receipt"
    );
    for version in ["-1", "-0", "18446744073709551616"] {
        fs::write(&path, format!("{{\"version\":{version}}}")).unwrap();
        assert_eq!(
            tabula::receipts::load_receipt(root, false)
                .unwrap_err()
                .message,
            "unsupported receipt version; use a compatible Tabula release"
        );
    }
}
