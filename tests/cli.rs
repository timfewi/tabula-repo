#![forbid(unsafe_code)]
use std::{fs, process::Command};

// Keep the reviewed legacy fixture immutable. The user-requested eighth command
// has exactly these additive help/choice-list differences; all other bytes and
// exit codes are still compared with the frozen CLI contract.
fn with_view_command(text: &str) -> String {
    text.replace(
        "{init,update,check,doctor,context,test-snapshot,import-test}",
        "{init,update,check,doctor,context,test-snapshot,import-test,view}",
    )
    .replace("'import-test')", "'import-test', 'view')")
    .replace(
        "    import-test         Record a caller-supplied test result from stdin.\n",
        concat!(
            "    import-test         Record a caller-supplied test result from stdin.\n",
            "    view                Generate an offline HTML table of the current guide state.\n",
        ),
    )
}

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
        let expected_out = with_view_command(case["stdout"].as_str().unwrap());
        let expected_err = with_view_command(case["stderr"].as_str().unwrap());
        if output.status.code().unwrap() != case["code"].as_i64().unwrap() as i32
            || output.stdout != expected_out.as_bytes()
            || output.stderr != expected_err.as_bytes()
        {
            failures.push(format!(
                "{:?}\nexpected code {} stdout {} stderr {}\nactual code {:?} stdout {} stderr {}",
                args,
                case["code"],
                expected_out,
                expected_err,
                output.status.code(),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
