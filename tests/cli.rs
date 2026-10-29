#![forbid(unsafe_code)]
use std::{fs, process::Command};
#[test]
fn cli_help_errors_and_options_match_frozen_reference() {
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("reference/cli.json")).unwrap();
    let mut failures = vec![];
    for case in cases.as_array().unwrap() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("fixture");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("README.md"), "source\n").unwrap();
        let args: Vec<_> = case["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        let output = Command::new(env!("CARGO_BIN_EXE_tabula"))
            .args(&args)
            .current_dir(root)
            .env("PATH", "")
            .output()
            .unwrap();
        if output.status.code().unwrap() != case["code"].as_i64().unwrap() as i32
            || output.stdout != case["stdout"].as_str().unwrap().as_bytes()
            || output.stderr != case["stderr"].as_str().unwrap().as_bytes()
        {
            failures.push(format!(
                "{:?}\nexpected code {} stdout {} stderr {}\nactual code {:?} stdout {} stderr {}",
                args,
                case["code"],
                case["stdout"],
                case["stderr"],
                output.status.code(),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
