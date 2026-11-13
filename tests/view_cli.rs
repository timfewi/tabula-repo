#![forbid(unsafe_code)]
use serde_json::Value;
use std::{fs, path::Path, process::Command};

fn run(root: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_tabula"))
        .args(args)
        .current_dir(root)
        .env("PATH", "")
        .output()
        .unwrap()
}

fn json(root: &Path, args: &[&str]) -> Value {
    let output = run(root, args);
    assert!(
        output.status.success(),
        "args {args:?}: stdout {} stderr {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(output.stderr.is_empty());
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn init_and_update_generate_an_ignored_view_and_preserve_existing_project_files() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("README.md"), "A source-backed project.\n").unwrap();
    fs::write(
        root.join("Justfile"),
        "# Existing task\ntest:\n    echo test\n",
    )
    .unwrap();
    fs::write(root.join(".gitignore"), "/cache/\n").unwrap();
    assert_eq!(
        json(root, &["init", ".", "--purpose", "Fixture", "--json"])["status"],
        "created"
    );
    let ignore = fs::read_to_string(root.join(".gitignore")).unwrap();
    assert!(ignore.starts_with("/cache/\n"));
    for name in ["/.tabula/state.json", "/.tabula/index.html"] {
        assert_eq!(ignore.lines().filter(|line| *line == name).count(), 1);
    }
    let recipes = fs::read_to_string(root.join("Justfile")).unwrap();
    assert!(recipes.starts_with("# Existing task\ntest:\n    echo test\n"));
    assert!(recipes.contains("tabula-view:"));
    assert!(recipes.contains("tabula view . --open"));
    assert!(root.join(".tabula/index.html").is_file());
    assert_eq!(json(root, &["update", ".", "--json"])["status"], "updated");
    assert_eq!(fs::read_to_string(root.join(".gitignore")).unwrap(), ignore);
    assert_eq!(fs::read_to_string(root.join("Justfile")).unwrap(), recipes);
}

#[test]
fn view_reports_stale_sources_without_updating_the_saved_baseline() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("README.md"), "Original source.\n").unwrap();
    json(root, &["init", "--purpose", "Fixture", "--json"]);
    let baseline = fs::read(root.join(".tabula/state.json")).unwrap();
    fs::write(root.join("README.md"), "Changed source.\n").unwrap();
    let result = json(root, &["view", ".", "--json"]);
    assert_eq!(result["status"], "rendered");
    assert_eq!(result["state"], "stale");
    assert_eq!(result["path"], ".tabula/index.html");
    assert_eq!(fs::read(root.join(".tabula/state.json")).unwrap(), baseline);
    assert_eq!(run(root, &["check", ".", "--json"]).status.code(), Some(1));
}

#[test]
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn unavailable_browser_preserves_the_successfully_generated_html() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("README.md"), "Source.\n").unwrap();
    json(root, &["init", "--purpose", "Fixture", "--json"]);
    let baseline = fs::read(root.join(".tabula/state.json")).unwrap();
    let result = json(root, &["view", ".", "--json", "--open"]);
    assert_eq!(result["status"], "rendered");
    assert_eq!(result["browser"], "unavailable");
    assert!(result["hint"]
        .as_str()
        .unwrap()
        .contains(".tabula/index.html"));
    assert!(root.join(".tabula/index.html").is_file());
    assert_eq!(fs::read(root.join(".tabula/state.json")).unwrap(), baseline);
}

#[test]
fn update_hashes_a_cited_justfile_after_adding_the_view_recipe() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir(root.join(".tabula")).unwrap();
    fs::write(root.join("Justfile"), "test:\n    echo test\n").unwrap();
    fs::write(
        root.join(".tabula/ENTRY.md"),
        "# Project\n\nThe [recipes](../Justfile) expose the project commands.\n",
    )
    .unwrap();
    json(root, &["update", ".", "--json"]);
    assert_eq!(json(root, &["check", ".", "--json"])["status"], "current");
    assert_eq!(json(root, &["view", ".", "--json"])["state"], "current");
}

#[test]
fn init_hashes_registered_topic_sources_after_adding_the_view_recipe() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir(root.join(".tabula")).unwrap();
    fs::write(root.join("README.md"), "Project declarations.\n").unwrap();
    fs::write(root.join("Justfile"), "test:\n    echo test\n").unwrap();
    fs::write(
        root.join(".tabula/cards.json"),
        r#"{"version":1,"cards":[{"id":"workflow","path":"workflow.md"}]}"#,
    )
    .unwrap();
    let topic = "# Workflow\n\nThe [recipes](../Justfile) expose project commands.\n";
    fs::write(root.join(".tabula/workflow.md"), topic).unwrap();
    json(root, &["init", ".", "--purpose", "Fixture", "--json"]);
    let recipes = fs::read(root.join("Justfile")).unwrap();
    assert!(String::from_utf8_lossy(&recipes).contains("tabula-view:"));
    let receipt: Value =
        serde_json::from_slice(&fs::read(root.join(".tabula/state.json")).unwrap()).unwrap();
    assert_eq!(
        receipt["sources"]["Justfile"],
        tabula::storage::digest(&recipes)
    );
    assert_eq!(json(root, &["check", ".", "--json"])["status"], "current");
    let html = fs::read_to_string(root.join(".tabula/index.html")).unwrap();
    assert!(html.contains("The guide and cited source bytes match the saved receipt."));
    assert_eq!(
        fs::read_to_string(root.join(".tabula/workflow.md")).unwrap(),
        topic
    );
}

fn refused_output_preserves_project(update: bool, symlink: bool) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("README.md"), "Original source.\n").unwrap();
    fs::write(root.join("Justfile"), "test:\n    echo test\n").unwrap();
    fs::write(root.join(".gitignore"), "/cache/\n").unwrap();
    if update {
        json(root, &["init", "--purpose", "Fixture", "--json"]);
        fs::remove_file(root.join(".tabula/index.html")).unwrap();
    } else {
        fs::create_dir(root.join(".tabula")).unwrap();
    }
    let baseline = fs::read(root.join(".tabula/state.json")).ok();
    let guide = fs::read(root.join(".tabula/ENTRY.md")).ok();
    let ignore = fs::read(root.join(".gitignore")).unwrap();
    let recipes = fs::read(root.join("Justfile")).unwrap();
    fs::write(root.join("README.md"), "Changed source.\n").unwrap();
    let outside = tempfile::NamedTempFile::new().unwrap();
    fs::write(outside.path(), "Unrelated output.\n").unwrap();
    if symlink {
        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.path(), root.join(".tabula/index.html")).unwrap();
        #[cfg(not(unix))]
        panic!("symlink fixture is only exercised on Unix");
    } else {
        fs::create_dir(root.join(".tabula/index.html")).unwrap();
    }
    let args: &[&str] = if update {
        &["update", ".", "--json"]
    } else {
        &["init", ".", "--purpose", "Fixture", "--json"]
    };
    let output = run(root, args);
    assert_eq!(output.status.code(), Some(2));
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["status"], "error");
    assert_eq!(result["kind"], "unsafe");
    assert_eq!(fs::read(root.join(".tabula/state.json")).ok(), baseline);
    assert_eq!(fs::read(root.join(".tabula/ENTRY.md")).ok(), guide);
    assert_eq!(fs::read(root.join(".gitignore")).unwrap(), ignore);
    assert_eq!(fs::read(root.join("Justfile")).unwrap(), recipes);
    assert_eq!(fs::read(outside.path()).unwrap(), b"Unrelated output.\n");
}

#[test]
fn init_and_update_refuse_a_directory_output_without_changing_project_files() {
    for update in [false, true] {
        refused_output_preserves_project(update, false);
    }
}

#[cfg(unix)]
#[test]
fn init_and_update_refuse_a_symlink_output_without_changing_project_files() {
    for update in [false, true] {
        refused_output_preserves_project(update, true);
    }
}

#[test]
fn view_argument_errors_and_missing_guide_do_not_write_project_files() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    for args in [
        vec!["view", ".", "--open=bad"],
        vec!["check", ".", "--open"],
        vec!["view", ".", "--unknown"],
        vec!["view", ".", "--json"],
    ] {
        assert_eq!(run(root, &args).status.code(), Some(2), "{args:?}");
        assert!(!root.join(".gitignore").exists());
        assert!(!root.join(".tabula").exists());
    }
    let output = run(root, &["view", "--help"]);
    assert!(output.status.success());
    assert!(String::from_utf8(output.stdout).unwrap().contains("--open"));
}
