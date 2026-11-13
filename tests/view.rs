#![forbid(unsafe_code)]
use std::fs;
use tabula::{operations, view};

fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("README.md"), "Original source\n").unwrap();
    fs::create_dir(dir.path().join(".tabula")).unwrap();
    fs::write(
        dir.path().join(".tabula/ENTRY.md"),
        "# Fixture\n\nA repository guide.\n\n## Main flow\n\nThe [source](../README.md) explains this fixture.\n",
    )
    .unwrap();
    dir
}

#[test]
fn view_observes_current_stale_and_missing_sources_without_updating_receipt() {
    let dir = fixture();
    let root = dir.path();
    operations::update(root, false, &[]).unwrap();
    let baseline = fs::read(root.join(".tabula/state.json")).unwrap();
    assert_eq!(view::write(root).unwrap()["state"], "current");
    fs::write(root.join("README.md"), "Changed source\n").unwrap();
    assert_eq!(view::write(root).unwrap()["state"], "stale");
    let html = fs::read_to_string(root.join(view::OUTPUT)).unwrap();
    assert!(html.contains("Source or guide bytes have changed"));
    assert!(html.contains("README.md"));
    assert!(html.contains("unreviewed"));
    fs::remove_file(root.join("README.md")).unwrap();
    assert_eq!(view::write(root).unwrap()["state"], "error");
    let html = fs::read_to_string(root.join(view::OUTPUT)).unwrap();
    assert!(html.contains("Some evidence could not be read or validated"));
    assert!(html.contains("diagnostic-kind\">missing"));
    assert_eq!(fs::read(root.join(".tabula/state.json")).unwrap(), baseline);
}

#[test]
fn view_without_baseline_and_with_malformed_receipt_remains_useful() {
    let dir = fixture();
    let root = dir.path();
    let result = view::write(root).unwrap();
    assert_eq!(result["state"], "unrecorded");
    assert!(!root.join(".tabula/state.json").exists());
    let html = fs::read_to_string(root.join(view::OUTPUT)).unwrap();
    assert!(html.contains("No saved receipt"));
    assert!(html.contains("Main flow"));
    assert!(!html.contains("<!--TABULA:"));
    fs::write(root.join(".tabula/state.json"), "{invalid").unwrap();
    assert_eq!(view::write(root).unwrap()["state"], "error");
    let html = fs::read_to_string(root.join(view::OUTPUT)).unwrap();
    assert!(html.contains("No valid receipt"));
    assert_eq!(
        fs::read(root.join(".tabula/state.json")).unwrap(),
        b"{invalid"
    );
}

#[test]
fn guide_markup_and_project_names_are_escaped() {
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().join("fixture & name");
    fs::create_dir(&root).unwrap();
    fs::create_dir(root.join(".tabula")).unwrap();
    fs::write(root.join("README.md"), "Source\n").unwrap();
    fs::write(
        root.join(".tabula/ENTRY.md"),
        "# Quoted & title\n\n`</pre><script>window.PWNED=true</script>`\n\n[source](../README.md)\n",
    )
    .unwrap();
    let result = view::write(&root).unwrap();
    assert_eq!(result["state"], "unrecorded");
    let html = fs::read_to_string(root.join(view::OUTPUT)).unwrap();
    assert!(html.contains("fixture &amp; name"));
    assert!(html.contains("&lt;/pre&gt;&lt;script&gt;window.PWNED=true&lt;/script&gt;"));
    assert!(!html.contains("<script>window.PWNED=true</script>"));
    assert!(!html.contains("fetch("));
    assert!(!html.contains("src=\"http"));
}

#[test]
fn review_and_test_assertions_remain_separate_from_freshness() {
    let dir = fixture();
    let root = dir.path();
    operations::update(root, false, &[]).unwrap();
    let report = operations::check(root).unwrap();
    let section = report["sections"]
        .as_object()
        .unwrap()
        .iter()
        .find(|(_, section)| section["heading"] == "Main flow")
        .unwrap()
        .0
        .clone();
    operations::update(root, false, &[section]).unwrap();
    let mut record = tabula::evidence::test_snapshot(root, &["README.md".into()]).unwrap();
    record["check_id"] = "fixture-test".into();
    record["outcome"] = "pass".into();
    record["provenance"] = "caller".into();
    tabula::evidence::import_test(root, &serde_json::to_vec(&record).unwrap()).unwrap();
    let baseline = fs::read(root.join(".tabula/state.json")).unwrap();
    view::write(root).unwrap();
    let html = fs::read_to_string(root.join(view::OUTPUT)).unwrap();
    assert!(html.contains("badge good\">acknowledged"));
    assert!(html.contains("fixture-test"));
    assert!(html.contains("caller assertion"));
    assert!(html.contains("badge good\">pass"));
    fs::write(root.join("README.md"), "Changed source\n").unwrap();
    assert_eq!(view::write(root).unwrap()["state"], "stale");
    let html = fs::read_to_string(root.join(view::OUTPUT)).unwrap();
    assert!(html.contains("badge attention\">invalidated"));
    assert!(html.contains("badge attention\">stale"));
    assert!(html.contains("badge good\">pass"));
    assert_eq!(fs::read(root.join(".tabula/state.json")).unwrap(), baseline);
}

#[cfg(unix)]
#[test]
fn view_refuses_symlink_output_without_changing_receipt_or_target() {
    let dir = fixture();
    let root = dir.path();
    operations::update(root, false, &[]).unwrap();
    let baseline = fs::read(root.join(".tabula/state.json")).unwrap();
    let output = root.join(view::OUTPUT);
    if output.exists() {
        fs::remove_file(&output).unwrap();
    }
    let outside = tempfile::NamedTempFile::new().unwrap();
    fs::write(outside.path(), "untouched").unwrap();
    std::os::unix::fs::symlink(outside.path(), &output).unwrap();
    assert_eq!(view::write(root).unwrap_err().kind, "unsafe");
    assert_eq!(fs::read(outside.path()).unwrap(), b"untouched");
    assert_eq!(fs::read(root.join(".tabula/state.json")).unwrap(), baseline);
}
