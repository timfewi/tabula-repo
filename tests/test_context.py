"""Actual CLI flows on small and several-topic fixture repositories."""

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from tabula import cards, limits, operations


class ContextTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.write("README.md", "Fixture declarations\n")
        self.write("research.py", "adapter = 1\n")
        self.write("desktop.py", "desktop = 1\n")
        self.write("shared.py", "shared = 1\n")
        operations.initialize(self.root, "Own the fixture.")

    def write(self, name, content):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content)

    def run_cli(self, command, *args, json_output=True, data=None):
        return subprocess.run(
            [
                sys.executable,
                str(Path(__file__).resolve().parents[1] / "tabula.py"),
                command,
                str(self.root),
                *(["--json"] if json_output else []),
                *args,
            ],
            cwd=self.root,
            input=data,
            capture_output=True,
            check=False,
        )

    def cli(self, command, *args, code=0, data=None):
        result = self.run_cli(command, *args, data=data)
        self.assertEqual(result.returncode, code, result.stdout + result.stderr)
        return json.loads(result.stdout)

    def registry(self, cards):
        self.write(".tabula/cards.json", json.dumps({"version": 1, "cards": cards}))

    def topics(self):
        self.registry(
            [
                {"id": "research", "path": "research.md"},
                {"id": "desktop", "path": "desktop.md"},
                {"id": "shared", "path": "shared.md"},
            ]
        )
        self.write(
            ".tabula/ENTRY.md",
            "# Fixture\nShared orientation. [declarations](../README.md)\n\n## Topics\n[Research](research.md) [Desktop](desktop.md) [Shared](shared.md)\n",
        )
        self.write(
            ".tabula/research.md",
            "# Research\nRetrieval orientation. [overview](../README.md)\n\n## Adapter\nThe adapter routes bounded requests. [implementation](../research.py)\n[shared](../shared.py)\n",
        )
        self.write(
            ".tabula/desktop.md",
            "# Desktop\nDesktop orientation. [overview](../README.md)\n\n## Session\nThe desktop owns windows. [implementation](../desktop.py)\n",
        )
        self.write(
            ".tabula/shared.md",
            "# Shared\nShared source boundary. [implementation](../shared.py)\n",
        )
        self.cli("update")

    def context(self, *files, budget=8000, code=0):
        result = self.run_cli("context", "--files", *files, "--max-bytes", str(budget))
        self.assertEqual(result.returncode, code, result.stdout + result.stderr)
        self.assertLessEqual(len(result.stdout), max(0, budget))
        return json.loads(result.stdout) if result.stdout else None

    def record(
        self,
        files=("research.py",),
        outcome="pass",
        provenance="caller",
        identity="research-unit",
    ):
        record = self.cli("test-snapshot", "--files", *files)
        record.update(check_id=identity, outcome=outcome, provenance=provenance)
        return record

    def test_small_legacy_single_card_needs_no_registration(self):
        current = json.loads((self.root / ".tabula/state.json").read_text())
        current = {
            "version": 1,
            "entry_sha256": current["entry_sha256"],
            "sources": current["sources"],
        }
        self.write(".tabula/state.json", json.dumps(current))
        result = self.context("README.md")
        self.assertTrue(result["sections"])
        self.assertTrue(all(s["review"] == "unreviewed" for s in result["sections"]))
        self.assertFalse((self.root / ".tabula/cards.json").exists())

    def test_research_change_spares_desktop_and_selects_complete_context(self):
        self.topics()
        sections = self.cli("check")["sections"]
        self.cli("update", "--review", *sections)
        record = self.record()
        self.cli("import-test", data=json.dumps(record).encode())
        self.write("research.py", "adapter = 2\n")
        changed = self.cli("check", code=1)
        self.assertEqual(changed["changed"], ["research.py"])
        self.assertEqual(changed["affected_sections"], ["research/2-adapter-1"])
        self.assertEqual(
            changed["sections"]["desktop/2-session-1"]["review"], "acknowledged"
        )
        result = self.context("research.py", budget=3500)
        identities = [section["id"] for section in result["sections"]]
        self.assertIn("research/2-adapter-1", identities)
        self.assertFalse(
            any(identity.startswith("desktop/") for identity in identities)
        )
        adapter = next(
            s for s in result["sections"] if s["id"] == "research/2-adapter-1"
        )
        self.assertEqual(adapter["freshness"], "stale")
        self.assertEqual(adapter["review"], "invalidated")
        self.assertEqual(
            adapter["text"],
            "## Adapter\nThe adapter routes bounded requests. [implementation](../research.py)\n[shared](../shared.py)\n",
        )
        self.assertEqual(result["tests"]["results"][0]["applicability"], "stale")
        self.assertEqual(result, self.context("research.py", budget=3500))
        self.cli("update")
        after = self.context("research.py")
        adapter = next(
            s for s in after["sections"] if s["id"] == "research/2-adapter-1"
        )
        self.assertEqual(adapter["freshness"], "current")
        self.assertEqual(adapter["review"], "unreviewed")
        self.assertEqual(after["tests"]["results"][0]["applicability"], "stale")

    def test_shared_dependencies_select_all_matching_sections_deterministically(self):
        self.topics()
        result = self.context("shared.py", "research.py")
        direct = [
            s["id"]
            for s in result["sections"]
            if s["reason"] == "exact source dependency"
        ]
        self.assertEqual(direct, ["research/2-adapter-1", "shared/1-shared-1"])
        self.assertEqual(
            result, self.context("research.py", "shared.py", "research.py")
        )

    def test_unknown_paths_and_empty_matches_are_explicit(self):
        result = self.context("not-cited.py")
        self.assertEqual(result["status"], "no_matches")
        self.assertEqual(result["unknown_paths"], ["not-cited.py"])
        self.assertEqual(result["sections"], [])
        result = self.context("README.md", "unknown.py")
        self.assertEqual(result["status"], "ok")
        self.assertEqual(result["unknown_paths"], ["unknown.py"])
        self.assertTrue(result["sections"])

    def test_complete_utf8_json_and_text_budgets_and_tiny_errors(self):
        self.write(
            ".tabula/ENTRY.md",
            "# Fixture\n" + "Größe 🦉. " * 100 + "[source](../README.md)\n",
        )
        self.cli("update")
        for json_output in (True, False):
            for budget in (1, 0, -1, 100, 500, 2000, 6000):
                result = self.run_cli(
                    "context",
                    "--files",
                    "README.md",
                    "--max-bytes",
                    str(budget),
                    json_output=json_output,
                )
                self.assertLessEqual(len(result.stdout), max(0, budget))
                if result.stdout and json_output:
                    parsed = json.loads(result.stdout)
                    if parsed.get("sections"):
                        self.assertEqual(
                            parsed["sections"][0]["text"],
                            (self.root / ".tabula/ENTRY.md").read_text(),
                        )
                if budget <= 100:
                    self.assertEqual(result.returncode, 2)
                    self.assertEqual(result.stdout, b"")
                    self.assertIn(b"byte budget", result.stderr)
        invalid = self.run_cli("context", "--files", "README.md", "--max-bytes", "many")
        self.assertEqual(invalid.returncode, 2)
        self.assertEqual(invalid.stdout, b"")

    def test_exact_output_boundary_and_whole_section_omission(self):
        full = self.run_cli("context", "--files", "README.md", "--max-bytes", "10000")
        same = self.run_cli(
            "context", "--files", "README.md", "--max-bytes", str(len(full.stdout))
        )
        self.assertEqual(full.stdout, same.stdout)
        less = self.context("README.md", budget=len(full.stdout) - 1)
        self.assertTrue(less["omitted"])
        self.assertLess(len(less["sections"]), len(json.loads(full.stdout)["sections"]))

    def test_missing_cyclic_duplicate_and_unregistered_cards_preserve_receipt(self):
        self.topics()
        receipt = self.root / ".tabula/state.json"
        before = receipt.read_bytes()
        self.write(
            ".tabula/research.md",
            "# Research\n[source](../research.py) [back](ENTRY.md)\n",
        )
        self.assertIn("cycle", self.cli("doctor", code=2)["diagnostics"][0]["message"])
        self.cli("update", code=2)
        self.assertEqual(receipt.read_bytes(), before)
        self.write(
            ".tabula/research.md",
            "# Research\n[source](../research.py) [missing](missing.md)\n",
        )
        self.cli("doctor", code=2)
        self.registry(
            [
                {"id": "duplicate", "path": "research.md"},
                {"id": "duplicate", "path": "desktop.md"},
            ]
        )
        self.assertIn(
            "duplicate", self.cli("doctor", code=2)["diagnostics"][0]["message"]
        )
        self.registry([{"id": "missing", "path": "missing.md"}])
        result = self.cli("doctor", code=2)
        self.assertTrue(any(d["kind"] == "missing" for d in result["diagnostics"]))
        self.cli("update", code=2)
        self.assertEqual(receipt.read_bytes(), before)

    def test_unregistered_files_are_never_read(self):
        self.write(".tabula/unregistered.md", "PRIVATE INVALID CARD")
        with (
            patch.object(Path, "rglob", side_effect=AssertionError("recursive scan")),
            patch.object(Path, "glob", side_effect=AssertionError("scan")),
        ):
            self.assertEqual(operations.doctor(self.root)["status"], "ok")
            operations.update(self.root)
        self.assertEqual(set(self.cli("doctor")["cards"]), {"entry"})

    def test_registration_path_symlink_and_depth_boundaries(self):
        (self.root / ".tabula/link.md").symlink_to(self.root / "README.md")
        for path in ("../README.md", "/etc/passwd.md", "link.md", "a/b/c/d/e.md"):
            with self.subTest(path=path):
                self.registry([{"id": "unsafe", "path": path}])
                self.cli("update", code=2)
        self.registry([{"id": f"card{i}", "path": f"card{i}.md"} for i in range(5)])
        for index in range(5):
            link = f" [next](card{index + 1}.md)" if index < 4 else ""
            self.write(
                f".tabula/card{index}.md", f"# Card\n[source](../README.md){link}\n"
            )
        self.assertTrue(
            any(
                "depth" in d["message"]
                for d in self.cli("doctor", code=2)["diagnostics"]
            )
        )

    def test_card_count_aggregate_and_global_source_limits(self):
        self.registry(
            [{"id": f"c{i}", "path": f"c{i}.md"} for i in range(limits.CARD_COUNT)]
        )
        self.cli("doctor", code=2)
        self.registry([{"id": f"c{i}", "path": f"c{i}.md"} for i in range(9)])
        for index in range(9):
            self.write(
                f".tabula/c{index}.md", "# Card\n[source](../README.md)\n" + "x" * 9500
            )
        result = self.cli("doctor", code=2)
        self.assertGreater(result["aggregate_card_bytes"], limits.CARD_TOTAL)
        self.assertTrue(any(d["kind"] == "oversized" for d in result["diagnostics"]))
        self.registry([{"id": "many", "path": "many.md"}])
        links = []
        for index in range(limits.SOURCE_COUNT):
            self.write(f"src{index}", "bytes")
            links.append(f"[source](../src{index})")
        self.write(".tabula/many.md", "# Many\n" + "\n".join(links))
        self.cli("update", code=2)

    def test_explicit_v2_upgrade_preserves_compatible_reviews(self):
        self.cli("update", "--review", "entry/2-source-entrypoints-1")
        path = self.root / ".tabula/state.json"
        state = json.loads(path.read_text())
        reviews = state["reviews"]
        state["version"] = 2
        del state["tests"]
        del state["registration_sha256"]
        self.write(".tabula/state.json", json.dumps(state))
        self.cli("check")
        self.cli("update", code=2)
        self.cli("update", "--upgrade")
        self.assertEqual(json.loads(path.read_text())["reviews"], reviews)

    def test_renamed_and_removed_topic_sections_are_reported(self):
        self.topics()
        self.write(
            ".tabula/research.md",
            "# Research\n[source](../README.md)\n\n## Retrieval\n[adapter](../research.py)\n",
        )
        result = self.cli("check", code=1)
        self.assertEqual(
            result["sections"]["research/2-adapter-1"]["freshness"], "removed"
        )
        self.assertEqual(
            result["sections"]["research/2-retrieval-1"]["review"], "unreviewed"
        )
        self.registry([{"id": "research", "path": "research.md"}])
        self.write(
            ".tabula/ENTRY.md",
            "# Fixture\n[source](../README.md) [topic](research.md)\n",
        )
        result = self.cli("check", code=1)
        self.assertIn(".tabula/desktop.md", result["changed"])
        self.assertIn(".tabula/cards.json", result["changed"])

    def test_test_evidence_missing_failed_untrusted_and_stale_are_distinct(self):
        self.topics()
        self.assertEqual(self.cli("check")["tests"]["status"], "missing")
        failed = self.record(outcome="fail")
        result = self.cli("import-test", data=json.dumps(failed).encode())
        self.assertEqual(result["tests"]["results"][0]["outcome"], "fail")
        self.assertEqual(self.cli("check")["status"], "current")
        unknown = self.record(
            outcome="pass", provenance="untrusted", identity="untrusted"
        )
        self.cli("import-test", data=json.dumps(unknown).encode())
        self.write("research.py", "changed")
        self.cli("update")
        records = self.cli("check")["tests"]["results"]
        self.assertTrue(all(record["applicability"] == "stale" for record in records))
        self.assertEqual(records[1]["provenance"], "untrusted")
        self.assertEqual(records[1]["outcome"], "pass")
        fresh = self.record()
        self.cli("import-test", data=json.dumps(fresh).encode())
        self.assertEqual(
            self.cli("check")["tests"]["results"][0]["applicability"], "current"
        )

    def test_import_does_not_refresh_card_or_acknowledge_review(self):
        self.topics()
        record = self.record()
        self.write("research.py", "changed after caller snapshot")
        result = self.cli("import-test", data=json.dumps(record).encode())
        self.assertEqual(result["tests"]["results"][0]["applicability"], "stale")
        result = self.cli("check", code=1)
        self.assertEqual(
            result["sections"]["research/2-adapter-1"]["review"], "unreviewed"
        )

    def test_malformed_test_metadata_and_raw_logs_are_rejected_atomically(self):
        self.topics()
        record = self.record()
        before = (self.root / ".tabula/state.json").read_bytes()
        malformed = [
            dict(record, logs="PRIVATE"),
            dict(record, provenance="verified"),
            dict(record, outcome="success"),
            dict(record, check_id="PRIVATE\nlog"),
            dict(record, sources={"../escape": "a" * 64}),
            dict(record, sources={".env": "a" * 64}),
            dict(record, sources={"research.py": "wrong"}),
            dict(record, sources={}),
        ]
        for item in malformed:
            with self.subTest(item=item):
                result = self.cli("import-test", data=json.dumps(item).encode(), code=2)
                self.assertNotIn("PRIVATE", json.dumps(result))
                self.assertEqual(
                    (self.root / ".tabula/state.json").read_bytes(), before
                )
        self.cli("import-test", data=b"x" * (limits.TEST_LIMIT + 1), code=2)
        self.cli("import-test", data=b'{"version":1,"version":1}', code=2)
        self.cli("import-test", data=b"[" * 2000, code=2)
        self.assertEqual((self.root / ".tabula/state.json").read_bytes(), before)

    def test_doctor_reports_affected_sections_and_related_context_links(self):
        self.topics()
        self.write("research.py", "changed")
        self.assertEqual(
            self.cli("doctor")["affected_sections"], ["research/2-adapter-1"]
        )
        result = self.context("research.py")
        self.assertEqual(
            [topic["id"] for topic in result["related_topics"]], ["research"]
        )
        self.assertEqual(result["omitted_topics"], [])

    def test_repeated_card_edges_are_bounded_and_sources_are_read_once(self):
        self.topics()
        self.write(
            ".tabula/ENTRY.md",
            "# Fixture\n[source](../README.md)\n" + "[topic](research.md) " * 300,
        )
        original = cards.read_file
        reads = []

        def observed(path, limit):
            reads.append(path.relative_to(self.root).as_posix())
            return original(path, limit)

        with patch.object(cards, "read_file", side_effect=observed):
            self.assertEqual(operations.doctor(self.root)["status"], "ok")
        self.assertEqual(reads.count("README.md"), 1)
        self.assertEqual(reads.count("shared.py"), 1)

    def test_test_result_limit_and_missing_test_inputs(self):
        self.topics()
        record = self.record()
        for index in range(limits.TEST_COUNT):
            record["check_id"] = f"check-{index:02}"
            self.cli("import-test", data=json.dumps(record).encode())
        before = (self.root / ".tabula/state.json").read_bytes()
        record["check_id"] = "overflow"
        self.cli("import-test", data=json.dumps(record).encode(), code=2)
        self.assertEqual((self.root / ".tabula/state.json").read_bytes(), before)
        (self.root / "research.py").unlink()
        result = self.cli("check", code=2)
        self.assertTrue(
            all(item["applicability"] == "stale" for item in result["tests"]["results"])
        )

    def test_unknown_receipt_fields_and_oversized_registry_are_rejected(self):
        path = self.root / ".tabula/state.json"
        state = json.loads(path.read_text())
        state["logs"] = "PRIVATE"
        self.write(".tabula/state.json", json.dumps(state))
        self.assertNotIn("PRIVATE", json.dumps(self.cli("doctor", code=2)))
        self.write(".tabula/cards.json", " " * (limits.REGISTRY_LIMIT + 1))
        result = self.cli("doctor", code=2)
        self.assertEqual(result["diagnostics"][0]["kind"], "oversized")

    def test_context_error_and_unknown_unsafe_paths_obey_output_budget(self):
        for target in ("../outside", "/etc/passwd", ".env"):
            self.context(target, budget=1, code=2)
        self.write(".tabula/state.json", "PRIVATE")
        self.context("README.md", budget=1, code=2)
        self.context("README.md", budget=1000, code=2)


if __name__ == "__main__":
    unittest.main()
