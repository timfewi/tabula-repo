#![forbid(unsafe_code)]
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::Value;
use std::{collections::BTreeSet, fs, path::Path};
use tabula::{context, operations};
fn bytes(v: &Value) -> Vec<u8> {
    if let Some(b) = v["base64"].as_str().or(v["bytes"].as_str()) {
        STANDARD.decode(b).unwrap()
    } else {
        v["repeat"]
            .as_str()
            .unwrap()
            .repeat(v["count"].as_u64().unwrap() as usize)
            .into_bytes()
    }
}
fn prepare(root: &Path, tree: &Value, blobs: &Value) {
    for (name, v) in tree.as_object().unwrap() {
        let path = root.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        if let Some(key) = v["blob"].as_str() {
            fs::write(path, bytes(&blobs[key])).unwrap();
        } else if v["directory"] == true {
            fs::create_dir_all(path).unwrap();
        } else if let Some(target) = v["symlink"].as_str() {
            let target = target.replace("@ROOT@", root.to_str().unwrap());
            #[cfg(unix)]
            std::os::unix::fs::symlink(target, path).unwrap();
            #[cfg(windows)]
            {
                if Path::new(&target).is_dir() {
                    std::os::windows::fs::symlink_dir(target, path).unwrap();
                } else {
                    std::os::windows::fs::symlink_file(target, path).unwrap();
                }
            }
        } else if v["fifo"] == true {
            #[cfg(unix)]
            {
                let status = std::process::Command::new("mkfifo")
                    .args(["-m", "600"])
                    .arg(&path)
                    .status()
                    .expect("POSIX mkfifo is required to prepare FIFO fixtures");
                assert!(status.success(), "could not create FIFO fixture");
            }
        }
    }
}
#[cfg(unix)]
#[test]
fn fifo_fixture_is_a_private_named_pipe() {
    use std::os::unix::fs::{FileTypeExt, PermissionsExt};
    let temp = tempfile::tempdir().unwrap();
    prepare(
        temp.path(),
        &serde_json::json!({"pipe": {"fifo": true}}),
        &Value::Null,
    );
    let metadata = fs::symlink_metadata(temp.path().join("pipe")).unwrap();
    assert!(metadata.file_type().is_fifo());
    assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
}
fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap().into())
        .collect()
}
fn inventory(root: &Path, directory: &Path, result: &mut BTreeSet<String>) {
    for entry in fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        result.insert(
            path.strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/"),
        );
        if fs::symlink_metadata(&path).unwrap().is_dir() {
            inventory(root, &path, result);
        }
    }
}
fn run_reference_test(test: &str) {
    let corpus: Value = serde_json::from_str(include_str!("reference/corpus.json")).unwrap();
    let mut failures = vec![];
    let cases = corpus["cases"].as_array().unwrap();
    let mut executed = 0;
    for (index, case) in cases
        .iter()
        .enumerate()
        .filter(|(_, case)| case["test"] == test)
    {
        #[cfg(windows)]
        if case["before"]
            .as_object()
            .unwrap()
            .values()
            .any(|v| v["fifo"] == true || v.get("symlink").is_some())
        {
            continue;
        }
        executed += 1;
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join(case["root_name"].as_str().unwrap());
        fs::create_dir(&root).unwrap();
        prepare(&root, &case["before"], &corpus["blobs"]);
        let op = case["operation"].as_str().unwrap();
        let args = &case["args"];
        let kwargs = &case["kwargs"];
        let result = match op {
            "initialize" => operations::initialize(&root, args[0].as_str().unwrap()),
            "update" => operations::update(
                &root,
                args[0]
                    .as_bool()
                    .or(kwargs["upgrade"].as_bool())
                    .unwrap_or(false),
                &args
                    .get(1)
                    .or(kwargs.get("acknowledge"))
                    .map(strings)
                    .unwrap_or_default(),
            ),
            "check" => operations::check(&root),
            "doctor" => operations::doctor(&root),
            "cli" => {
                let args = strings(args)
                    .iter()
                    .map(|s| s.replace("@ROOT@", root.to_str().unwrap()))
                    .collect::<Vec<_>>();
                let input = bytes(&case["input"]);
                let (code, out, err) = tabula::cli::run(&args, &input);
                let expected_out = case["stdout"].as_str().unwrap();
                let expected_err = case["stderr"].as_str().unwrap();
                if code != case["code"].as_i64().unwrap() as i32
                    || out != expected_out
                    || err != expected_err
                {
                    failures.push(format!("case {index} cli {:?}\nexpected code {} out {} err {}\nactual code {code} out {out} err {err}",args,case["code"],expected_out,expected_err));
                }
                Ok(Value::Null)
            }
            _ => panic!("unknown operation {op}"),
        };
        if op != "cli" {
            match result {
                Ok(v) => {
                    if case.get("error").is_some()
                        || context::render(&v, true) != case["json"].as_str().unwrap_or("")
                        || context::render(&v, false) != case["text"].as_str().unwrap_or("")
                    {
                        failures.push(format!(
                            "case {index} {op}: expected {} error {} actual {}",
                            case["result"], case["error"], v
                        ));
                    }
                }
                Err(e) => {
                    if e.diagnostic(None, None) != case["error"] {
                        failures.push(format!(
                            "case {index} {op}: expected {} error {} actual {:?}",
                            case["result"], case["error"], e
                        ));
                    }
                }
            }
        }
        let mut actual_paths = BTreeSet::new();
        inventory(&root, &root, &mut actual_paths);
        let expected_paths = case["after"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>();
        if actual_paths != expected_paths {
            failures.push(format!("case {index} {op}: filesystem inventory differs"));
        }
        for (name, v) in case["after"].as_object().unwrap() {
            if let Some(key) = v["blob"].as_str() {
                if fs::read(root.join(name)).ok() != Some(bytes(&corpus["blobs"][key])) {
                    failures.push(format!("case {index} {op}: written bytes differ at {name}"));
                }
            }
        }
        if failures.len() > 12 {
            break;
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    eprintln!("{executed} frozen operation and CLI cases passed; {} POSIX-only cases not applicable on this platform", cases.iter().filter(|case|case["test"] == test).count() - executed);
}

macro_rules! reference_test {
    ($name:ident, $reference:literal) => {
        #[test]
        fn $name() {
            run_reference_test($reference);
        }
    };
}
reference_test!(
    test_tabula_test_broken_link_does_not_replace_receipt,
    "test_tabula.test_broken_link_does_not_replace_receipt"
);
reference_test!(
    test_tabula_test_card_edits_are_stale_until_reviewed_update,
    "test_tabula.test_card_edits_are_stale_until_reviewed_update"
);
reference_test!(
    test_tabula_test_cli_status_is_machine_readable_and_does_not_execute_repo_code,
    "test_tabula.test_cli_status_is_machine_readable_and_does_not_execute_repo_code"
);
reference_test!(
    test_tabula_test_init_check_and_update_preserve_explanations,
    "test_tabula.test_init_check_and_update_preserve_explanations"
);
reference_test!(
    test_tabula_test_links_are_relative_and_encoded_spaces_work,
    "test_tabula.test_links_are_relative_and_encoded_spaces_work"
);
reference_test!(
    test_tabula_test_malformed_receipt_returns_error_without_echoing_content,
    "test_tabula.test_malformed_receipt_returns_error_without_echoing_content"
);
reference_test!(
    test_tabula_test_path_escapes_credentials_state_and_symlinks_are_rejected,
    "test_tabula.test_path_escapes_credentials_state_and_symlinks_are_rejected"
);
reference_test!(
    test_tabula_test_special_and_large_files_are_bounded,
    "test_tabula.test_special_and_large_files_are_bounded"
);
reference_test!(
    test_tabula_test_state_symlink_cannot_redirect_writes,
    "test_tabula.test_state_symlink_cannot_redirect_writes"
);
reference_test!(
    test_context_test_card_count_aggregate_and_global_source_limits,
    "test_context.test_card_count_aggregate_and_global_source_limits"
);
reference_test!(
    test_context_test_complete_utf8_json_and_text_budgets_and_tiny_errors,
    "test_context.test_complete_utf8_json_and_text_budgets_and_tiny_errors"
);
reference_test!(
    test_context_test_context_error_and_unknown_unsafe_paths_obey_output_budget,
    "test_context.test_context_error_and_unknown_unsafe_paths_obey_output_budget"
);
reference_test!(
    test_context_test_doctor_reports_affected_sections_and_related_context_links,
    "test_context.test_doctor_reports_affected_sections_and_related_context_links"
);
reference_test!(
    test_context_test_exact_output_boundary_and_whole_section_omission,
    "test_context.test_exact_output_boundary_and_whole_section_omission"
);
reference_test!(
    test_context_test_explicit_v2_upgrade_preserves_compatible_reviews,
    "test_context.test_explicit_v2_upgrade_preserves_compatible_reviews"
);
reference_test!(
    test_context_test_import_does_not_refresh_card_or_acknowledge_review,
    "test_context.test_import_does_not_refresh_card_or_acknowledge_review"
);
reference_test!(
    test_context_test_malformed_test_metadata_and_raw_logs_are_rejected_atomically,
    "test_context.test_malformed_test_metadata_and_raw_logs_are_rejected_atomically"
);
reference_test!(
    test_context_test_missing_cyclic_duplicate_and_unregistered_cards_preserve_receipt,
    "test_context.test_missing_cyclic_duplicate_and_unregistered_cards_preserve_receipt"
);
reference_test!(
    test_context_test_registration_path_symlink_and_depth_boundaries,
    "test_context.test_registration_path_symlink_and_depth_boundaries"
);
reference_test!(
    test_context_test_renamed_and_removed_topic_sections_are_reported,
    "test_context.test_renamed_and_removed_topic_sections_are_reported"
);
reference_test!(
    test_context_test_repeated_card_edges_are_bounded_and_sources_are_read_once,
    "test_context.test_repeated_card_edges_are_bounded_and_sources_are_read_once"
);
reference_test!(
    test_context_test_research_change_spares_desktop_and_selects_complete_context,
    "test_context.test_research_change_spares_desktop_and_selects_complete_context"
);
reference_test!(
    test_context_test_shared_dependencies_select_all_matching_sections_deterministically,
    "test_context.test_shared_dependencies_select_all_matching_sections_deterministically"
);
reference_test!(
    test_context_test_small_legacy_single_card_needs_no_registration,
    "test_context.test_small_legacy_single_card_needs_no_registration"
);
reference_test!(
    test_context_test_test_evidence_missing_failed_untrusted_and_stale_are_distinct,
    "test_context.test_test_evidence_missing_failed_untrusted_and_stale_are_distinct"
);
reference_test!(
    test_context_test_test_result_limit_and_missing_test_inputs,
    "test_context.test_test_result_limit_and_missing_test_inputs"
);
reference_test!(
    test_context_test_unknown_paths_and_empty_matches_are_explicit,
    "test_context.test_unknown_paths_and_empty_matches_are_explicit"
);
reference_test!(
    test_context_test_unknown_receipt_fields_and_oversized_registry_are_rejected,
    "test_context.test_unknown_receipt_fields_and_oversized_registry_are_rejected"
);
reference_test!(
    test_context_test_unregistered_files_are_never_read,
    "test_context.test_unregistered_files_are_never_read"
);
reference_test!(
    test_evidence_test_crlf_and_multiline_link_labels_preserve_evidence,
    "test_evidence.test_crlf_and_multiline_link_labels_preserve_evidence"
);
reference_test!(
    test_evidence_test_doctor_unicode_size_without_reading_oversized_card,
    "test_evidence.test_doctor_unicode_size_without_reading_oversized_card"
);
reference_test!(
    test_evidence_test_duplicate_headings_and_setext_sections,
    "test_evidence.test_duplicate_headings_and_setext_sections"
);
reference_test!(
    test_evidence_test_examples_are_not_evidence,
    "test_evidence.test_examples_are_not_evidence"
);
reference_test!(
    test_evidence_test_legacy_receipt_requires_explicit_atomic_upgrade,
    "test_evidence.test_legacy_receipt_requires_explicit_atomic_upgrade"
);
reference_test!(
    test_evidence_test_malformed_section_identity_is_sanitized,
    "test_evidence.test_malformed_section_identity_is_sanitized"
);
reference_test!(
    test_evidence_test_markdown_code_delimiters_entities_and_reference_normalization,
    "test_evidence.test_markdown_code_delimiters_entities_and_reference_normalization"
);
reference_test!(
    test_evidence_test_missing_and_oversized_sources_report_sections,
    "test_evidence.test_missing_and_oversized_sources_report_sections"
);
reference_test!(
    test_evidence_test_reference_source_change_is_not_missed,
    "test_evidence.test_reference_source_change_is_not_missed"
);
reference_test!(
    test_evidence_test_required_markdown_links_and_titles,
    "test_evidence.test_required_markdown_links_and_titles"
);
reference_test!(
    test_evidence_test_review_unknown_section_does_not_write,
    "test_evidence.test_review_unknown_section_does_not_write"
);
reference_test!(
    test_evidence_test_section_reviews_shared_dependencies_removal_and_rename,
    "test_evidence.test_section_reviews_shared_dependencies_removal_and_rename"
);
reference_test!(
    test_evidence_test_source_and_section_counts_are_bounded,
    "test_evidence.test_source_and_section_counts_are_bounded"
);
reference_test!(
    test_evidence_test_unknown_and_malformed_receipts_are_safe_and_preserved,
    "test_evidence.test_unknown_and_malformed_receipts_are_safe_and_preserved"
);
reference_test!(
    test_evidence_test_unpaired_code_does_not_hide_evidence_across_paragraphs,
    "test_evidence.test_unpaired_code_does_not_hide_evidence_across_paragraphs"
);
reference_test!(
    test_evidence_test_unrelated_prose_edits_preserve_review_and_inline_html_is_code,
    "test_evidence.test_unrelated_prose_edits_preserve_review_and_inline_html_is_code"
);
reference_test!(
    test_evidence_test_unsupported_nested_and_indented_paragraph_links_fail_explicitly,
    "test_evidence.test_unsupported_nested_and_indented_paragraph_links_fail_explicitly"
);
reference_test!(
    test_evidence_test_unsupported_or_malformed_evidence_has_line_diagnostics,
    "test_evidence.test_unsupported_or_malformed_evidence_has_line_diagnostics"
);
