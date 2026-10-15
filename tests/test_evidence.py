"""Behavioral contracts for Markdown evidence, diagnosis and review."""

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

import tabula


class EvidenceTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.source("README.md", "readme\n")
        self.source("adapter.py", "before\n")
        self.source("desktop.py", "desktop\n")
        tabula.initialize(self.root, "Fixture")

    def source(self, name, content):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content)

    def card(self, content):
        self.source(".tabula/ENTRY.md", content)

    def cli(self, command, *options, code=0):
        result = subprocess.run(
            [
                sys.executable,
                tabula.__file__,
                command,
                str(self.root),
                "--json",
                *options,
            ],
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(result.returncode, code, result.stdout + result.stderr)
        return json.loads(result.stdout)

    def receipt(self):
        return (self.root / ".tabula/state.json").read_bytes()

    def sections(self):
        return tabula.check(self.root)["sections"]

    def test_reference_source_change_is_not_missed(self):
        self.card(
            '# Fixture\n[base](../README.md)\n\n## Research\n[adapter][impl]\n\n[impl]: ../adapter.py "implementation"\n'
        )
        tabula.update(self.root)
        self.source("adapter.py", "changed\n")
        result = self.cli("check", code=1)
        self.assertEqual(result["changed"], ["adapter.py"])
        self.assertEqual(result["affected_sections"], ["entry/2-research-1"])

    def test_required_markdown_links_and_titles(self):
        self.source("a(b).py", "balanced")
        self.source("space file.py", "space")
        self.card("""# Fixture
[inline](../a(b).py "title")
[escaped](../a\\(b\\).py 'title')
[angle](<../space file.py#fragment> (title))
[percent](../space%20file.py#L1)
[full][Destination]
[collapsed][]
[shortcut]
[escaped\\]label](../README.md)

[Destination]: <../adapter.py> "definition title"
[collapsed]: ../desktop.py
[shortcut]: ../README.md
""")
        self.cli("update")
        state = json.loads(self.receipt())
        self.assertEqual(
            set(state["sources"]),
            {"README.md", "adapter.py", "desktop.py", "a(b).py", "space file.py"},
        )

    def test_examples_are_not_evidence(self):
        self.card("""# Fixture
[real](../README.md)
```markdown
[example](../missing.py)
````
~~~
[another][undefined]
~~~

    [indented](../missing.py)

`[inline](../missing.py)` and ``code ` [other](../missing.py)``.
\\[literal\\] and [remote](https://example.invalid/no-network).
""")
        self.cli("update")
        result = self.cli("doctor")
        self.assertEqual(result["evidence_files"], 1)
        self.assertEqual(
            result["cards"]["entry"]["navigation"], [{"line": 13, "kind": "external"}]
        )

    def test_unsupported_or_malformed_evidence_has_line_diagnostics(self):
        for example in (
            "[missing][undefined]",
            '[oops](../adapter.py "unclosed)',
            "![image](../README.md)",
            '<a href="../adapter.py">source</a>',
            "> [source](../adapter.py)",
            "[x](../a(b.py)",
            "[x]: ../adapter.py\n[x]: ../desktop.py",
        ):
            with self.subTest(example=example):
                self.card("# Fixture\n[base](../README.md)\n" + example + "\n")
                before = self.receipt()
                result = self.cli("doctor", code=2)
                self.assertEqual(result["diagnostics"][0]["kind"], "malformed")
                self.assertEqual(result["diagnostics"][0]["file"], ".tabula/ENTRY.md")
                self.assertGreaterEqual(result["diagnostics"][0]["line"], 3)
                self.cli("update", code=2)
                self.assertEqual(self.receipt(), before)

    def test_section_reviews_shared_dependencies_removal_and_rename(self):
        card = "# Fixture\n[base](../README.md)\n\n## Research\n[adapter](../adapter.py)\n\n## Shared\n[adapter](../adapter.py)\n\n## Desktop\n[desktop](../desktop.py)\n"
        self.card(card)
        self.cli("update")
        identities = list(self.sections())
        self.cli("update", "--review", *identities)
        self.source("adapter.py", "after\n")
        result = self.cli("check", code=1)
        self.assertEqual(
            set(result["affected_sections"]), {"entry/2-research-1", "entry/2-shared-1"}
        )
        self.assertEqual(
            result["sections"]["entry/2-desktop-1"]["review"], "acknowledged"
        )
        self.assertEqual(
            result["sections"]["entry/2-research-1"]["review"], "invalidated"
        )
        self.cli("update")
        result = self.cli("check")
        self.assertEqual(
            result["sections"]["entry/2-research-1"]["review"], "unreviewed"
        )
        self.assertEqual(
            result["sections"]["entry/2-desktop-1"]["review"], "acknowledged"
        )
        self.card(card.replace("[adapter](../adapter.py)", "No dependency.", 1))
        result = self.cli("check", code=1)
        self.assertIn("entry/2-research-1", result["affected_sections"])
        self.card(card.replace("## Research", "## Retrieval"))
        result = self.cli("check", code=1)
        self.assertEqual(
            result["sections"]["entry/2-research-1"]["freshness"], "removed"
        )
        self.assertEqual(
            result["sections"]["entry/2-retrieval-1"]["review"], "unreviewed"
        )

    def test_duplicate_headings_and_setext_sections(self):
        self.card(
            "# Fixture\n[base](../README.md)\n\nSame\n----\n[first](../adapter.py)\n\n## Same\n[second](../desktop.py)\n"
        )
        self.cli("update")
        self.source("desktop.py", "changed")
        result = self.cli("check", code=1)
        self.assertEqual(result["affected_sections"], ["entry/2-same-2"])

    def test_legacy_receipt_requires_explicit_atomic_upgrade(self):
        state = json.loads(self.receipt())
        legacy = {key: state[key] for key in ("entry_sha256", "sources")}
        legacy["version"] = 1
        self.source(".tabula/state.json", json.dumps(legacy))
        before = self.receipt()
        card = (self.root / ".tabula/ENTRY.md").read_bytes()
        result = self.cli("check")
        self.assertEqual(result["receipt_version"], 1)
        self.assertTrue(
            all(s["review"] == "unreviewed" for s in result["sections"].values())
        )
        self.cli("update", code=2)
        self.assertEqual(before, self.receipt())
        self.cli("update", "--upgrade")
        self.assertEqual((self.root / ".tabula/ENTRY.md").read_bytes(), card)
        self.assertEqual(json.loads(self.receipt())["reviews"], {})

    def test_unknown_and_malformed_receipts_are_safe_and_preserved(self):
        for state in ({"version": 300}, {"version": 2}, ["PRIVATE"], {"version": True}):
            with self.subTest(state=state):
                self.source(".tabula/state.json", json.dumps(state))
                before = self.receipt()
                result = self.cli("check", code=2)
                self.assertNotIn("PRIVATE", json.dumps(result))
                self.cli("update", code=2)
                self.assertEqual(before, self.receipt())
                doctor = self.cli("doctor", code=2)
                self.assertEqual(doctor["evidence_files"], 1)
                self.assertGreater(doctor["cards"]["entry"]["bytes"], 0)

    def test_doctor_unicode_size_without_reading_oversized_card(self):
        self.card("# Fixture\n" + "ä" * 5000)
        result = self.cli("doctor", code=2)
        card = result["cards"]["entry"]
        self.assertEqual(card["bytes"], 10010)
        self.assertEqual(card["exceeded_bytes"], 10)
        self.assertEqual(card["remaining_bytes"], 0)
        self.assertEqual(result["diagnostics"][0]["kind"], "oversized")

    def test_missing_and_oversized_sources_report_sections(self):
        self.card(
            "# Fixture\n[base](../README.md)\n\n## Research\n[adapter](../adapter.py)\n"
        )
        self.cli("update")
        (self.root / "adapter.py").unlink()
        result = self.cli("check", code=2)
        self.assertIn("adapter.py", result["changed"])
        self.assertIn("entry/2-research-1", result["affected_sections"])
        self.assertEqual(result["diagnostics"][0]["kind"], "missing")
        self.source("adapter.py", "x" * (tabula.SOURCE_LIMIT + 1))
        self.assertEqual(
            self.cli("doctor", code=2)["diagnostics"][0]["kind"], "oversized"
        )

    def test_source_and_section_counts_are_bounded(self):
        links = []
        for index in range(tabula.SOURCE_COUNT + 1):
            self.source(f"s{index}", "source")
            links.append(f"[s](../s{index})")
        self.card("# Fixture\n" + "\n".join(links))
        result = self.cli("doctor", code=2)
        self.assertEqual(result["diagnostics"][0]["kind"], "oversized")
        self.card("# Fixture\n[base](../README.md)\n" + "## S\n" * tabula.SECTION_COUNT)
        self.cli("update", code=2)

    def test_unrelated_prose_edits_preserve_review_and_inline_html_is_code(self):
        self.card(
            "# Fixture\n[base](../README.md)\n\n## Research\n[adapter](../adapter.py)\n\n## Desktop\n[desktop](../desktop.py)\n"
        )
        self.cli("update", "--review", "entry/2-desktop-1")
        content = (self.root / ".tabula/ENTRY.md").read_text()
        self.card(
            content.replace(
                "## Research\n",
                '## Research\nExtra explanation.\n`<a href="secret">example</a>`\n',
            )
        )
        self.cli("update")
        self.assertEqual(self.sections()["entry/2-desktop-1"]["review"], "acknowledged")

    def test_markdown_code_delimiters_entities_and_reference_normalization(self):
        self.source("a&b.py", "entity destination")
        self.source("back`tick`.py", "literal backticks in destination")
        self.card(
            "# Fixture\n[base](../README.md)\n"
            "[label `] [hidden](../absent)`](../adapter.py)\n"
            "[entity](../a&amp;b.py)\n"
            "[ticks](../back`tick`.py)\n"
            "[ref][ Mixed   CASE ]\n\n[mixed case]: ../desktop.py\n"
        )
        self.cli("update")
        self.assertEqual(
            set(json.loads(self.receipt())["sources"]),
            {"README.md", "adapter.py", "desktop.py", "a&b.py", "back`tick`.py"},
        )

    def test_unpaired_code_does_not_hide_evidence_across_paragraphs(self):
        self.card(
            "# Fixture\nUnpaired ` literal.\n\n[adapter](../adapter.py)\n\nAnother ` literal.\n"
        )
        self.cli("update")
        self.source("adapter.py", "changed")
        self.assertEqual(self.cli("check", code=1)["changed"], ["adapter.py"])

    def test_unsupported_nested_and_indented_paragraph_links_fail_explicitly(self):
        for body in (
            "- item\n    [link](../adapter.py)\n",
            "paragraph\n    [link](../adapter.py)\n",
            "[outer [inner](../adapter.py)](../README.md)\n",
        ):
            self.card("# Fixture\n[base](../README.md)\n\n" + body)
            result = self.cli("doctor", code=2)
            self.assertEqual(result["diagnostics"][0]["kind"], "malformed")

    def test_crlf_and_multiline_link_labels_preserve_evidence(self):
        self.card(
            "# Fixture\r\nUnpaired ` literal.\r\n\r\n[source\r\nlabel](../adapter.py)\r\n\r\nAnother ` literal.\r\n"
        )
        self.cli("update")
        self.source("adapter.py", "changed")
        self.assertEqual(self.cli("check", code=1)["changed"], ["adapter.py"])

    def test_malformed_section_identity_is_sanitized(self):
        state = json.loads(self.receipt())
        sections = state["cards"]["entry"]["sections"]
        sections["entry/\ud800"] = sections.pop(next(iter(sections)))
        self.source(".tabula/state.json", json.dumps(state))
        self.assertEqual(self.cli("check", code=2)["kind"], "malformed")

    def test_review_unknown_section_does_not_write(self):
        before = self.receipt()
        self.cli("update", "--review", "absent", code=2)
        self.assertEqual(before, self.receipt())


if __name__ == "__main__":
    unittest.main()
