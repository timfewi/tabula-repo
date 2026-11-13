#![forbid(unsafe_code)]
use std::fs;
use tabula::project;

#[test]
fn integration_preserves_files_and_is_idempotent() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let original_ignore = "/target/\n# Owner comment\n/.tabula/state.json\n";
    let original_just = "set shell := [\"sh\", \"-eu\", \"-c\"]\n\ncheck:\n    true\n";
    fs::write(root.join(".gitignore"), original_ignore).unwrap();
    fs::write(root.join("Justfile"), original_just).unwrap();
    project::prepare(root).unwrap();
    let ignore = fs::read_to_string(root.join(".gitignore")).unwrap();
    let just = fs::read_to_string(root.join("Justfile")).unwrap();
    assert!(ignore.starts_with(original_ignore));
    assert_eq!(ignore.matches("/.tabula/state.json").count(), 1);
    assert_eq!(ignore.matches("/.tabula/index.html").count(), 1);
    assert!(just.starts_with(original_just));
    assert!(just.contains("tabula-view:\n    @tabula view . --open\n"));
    project::prepare(root).unwrap();
    assert_eq!(fs::read_to_string(root.join(".gitignore")).unwrap(), ignore);
    assert_eq!(fs::read_to_string(root.join("Justfile")).unwrap(), just);
}

#[test]
fn missing_justfile_is_not_created() {
    let temp = tempfile::tempdir().unwrap();
    project::prepare(temp.path()).unwrap();
    let ignore = fs::read_to_string(temp.path().join(".gitignore")).unwrap();
    assert!(ignore.contains("/.tabula/state.json\n/.tabula/index.html\n"));
    for name in ["Justfile", "justfile", ".justfile"] {
        assert!(!temp.path().join(name).exists());
    }
}

#[test]
fn later_negations_are_overridden_without_repeated_appends() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let original = "/.tabula/state.json\n/.tabula/index.html\n!/.tabula/*\n";
    fs::write(root.join(".gitignore"), original).unwrap();
    project::prepare(root).unwrap();
    let ignore = fs::read_to_string(root.join(".gitignore")).unwrap();
    assert!(ignore.starts_with(original));
    assert!(ignore.ends_with("/.tabula/state.json\n/.tabula/index.html\n"));
    project::prepare(root).unwrap();
    assert_eq!(fs::read_to_string(root.join(".gitignore")).unwrap(), ignore);
}

#[test]
fn crlf_and_lowercase_or_hidden_justfiles_are_supported() {
    for name in ["justfile", ".justfile"] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::write(root.join(".gitignore"), "/target/\r\n").unwrap();
        fs::write(root.join(name), "check:\r\n    true\r\n").unwrap();
        project::prepare(root).unwrap();
        for file in [".gitignore", name] {
            let text = fs::read_to_string(root.join(file)).unwrap();
            assert!(text.contains("\r\n"));
            assert!(!text.replace("\r\n", "").contains('\n'));
        }
    }
}

#[test]
fn existing_view_recipes_and_aliases_keep_their_owner() {
    for source in [
        "tabula-view:\n    custom-view\n",
        "[private]\ntabula-view *args:\n    custom-view {{args}}\n",
        "alias tabula-view := inspect\ninspect:\n    custom-view\n",
        "@tabula-view:\n    custom-view\n",
    ] {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("Justfile"), source).unwrap();
        project::prepare(temp.path()).unwrap();
        assert_eq!(
            fs::read_to_string(temp.path().join("Justfile")).unwrap(),
            source
        );
    }
}

#[test]
fn variable_named_like_the_view_does_not_hide_the_recipe() {
    for source in [
        "tabula-view := 'value'\n",
        "tabula-view:='https://example.invalid:443'\n",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("Justfile");
        fs::write(&path, source).unwrap();
        project::prepare(temp.path()).unwrap();
        let integrated = fs::read_to_string(&path).unwrap();
        assert!(integrated.starts_with(source));
        assert!(integrated.contains("\ntabula-view:\n    @tabula view . --open\n"));
        project::prepare(temp.path()).unwrap();
        assert_eq!(fs::read_to_string(path).unwrap(), integrated);
    }
}

#[test]
fn invalid_justfile_prevents_even_the_planned_ignore_write() {
    for invalid in [vec![0xff, 0xfe], vec![b'x'; 1024 * 1024 + 1]] {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("Justfile"), invalid).unwrap();
        assert!(project::prepare(temp.path()).is_err());
        assert!(!temp.path().join(".gitignore").exists());
    }
}

#[test]
fn directory_targets_are_refused_before_mutation() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("Justfile")).unwrap();
    assert!(project::prepare(temp.path()).is_err());
    assert!(!temp.path().join(".gitignore").exists());
}

#[test]
fn invalid_output_path_prevents_integration_writes() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join(".tabula/index.html")).unwrap();
    assert!(project::prepare(temp.path()).is_err());
    assert!(!temp.path().join(".gitignore").exists());
}

#[cfg(unix)]
#[test]
fn symlink_targets_are_refused_before_mutation() {
    use std::os::unix::fs::symlink;
    for target in [".gitignore", "Justfile", ".tabula/index.html"] {
        let temp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let original = b"keep this file\n";
        fs::write(outside.path().join("source"), original).unwrap();
        if target.starts_with(".tabula/") {
            fs::create_dir(temp.path().join(".tabula")).unwrap();
        }
        symlink(outside.path().join("source"), temp.path().join(target)).unwrap();
        assert!(project::prepare(temp.path()).is_err());
        assert_eq!(fs::read(outside.path().join("source")).unwrap(), original);
        if target != ".gitignore" {
            assert!(!temp.path().join(".gitignore").exists());
        }
    }
}

#[cfg(unix)]
#[test]
fn existing_file_permissions_are_preserved() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    for name in [".gitignore", "Justfile"] {
        fs::write(root.join(name), "# Existing project file\n").unwrap();
        fs::set_permissions(root.join(name), fs::Permissions::from_mode(0o640)).unwrap();
    }
    project::prepare(root).unwrap();
    for name in [".gitignore", "Justfile"] {
        assert_eq!(
            fs::metadata(root.join(name)).unwrap().permissions().mode() & 0o777,
            0o640
        );
    }
}
