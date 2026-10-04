import contextlib
import io
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

import tabula


class TabulaTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        (self.root / "README.md").write_text("# Fixture\n\nA source fixture.\n")

    def init(self):
        tabula.initialize(self.root, "Own the fixture.")

    def test_init_check_and_update_preserve_explanations(self):
        self.init()
        self.assertEqual(tabula.check(self.root)["status"], "current")
        entry = self.root / ".tabula/ENTRY.md"
        text = entry.read_bytes()
        (self.root / "README.md").write_text("# Changed source\n")
        self.assertEqual(tabula.check(self.root)["changed"], ["README.md"])
        tabula.update(self.root)
        self.assertEqual(entry.read_bytes(), text)
        self.assertEqual(tabula.check(self.root)["status"], "current")
        with self.assertRaisesRegex(ValueError, "already exists"):
            self.init()
        self.assertEqual(entry.read_bytes(), text)

    def test_card_edits_are_stale_until_reviewed_update(self):
        self.init()
        entry = self.root / ".tabula/ENTRY.md"
        entry.write_text(entry.read_text() + "\nA reviewed architecture explanation.\n")
        self.assertEqual(tabula.check(self.root)["changed"], [".tabula/ENTRY.md"])
        tabula.update(self.root)
        self.assertEqual(tabula.check(self.root)["status"], "current")

    def test_broken_link_does_not_replace_receipt(self):
        self.init()
        receipt = self.root / ".tabula/state.json"
        previous = receipt.read_bytes()
        (self.root / "README.md").unlink()
        with self.assertRaises(FileNotFoundError):
            tabula.update(self.root)
        self.assertEqual(receipt.read_bytes(), previous)

    def test_links_are_relative_and_encoded_spaces_work(self):
        self.init()
        (self.root / "source file.py").write_text("value = 1\n")
        (self.root / ".tabula/ENTRY.md").write_text(
            "# Fixture\n[code](../source%20file.py#L1)\n"
            "[remote](https://example.com/private) [section](#local)\n"
            "[folder](../tests/)\n"
        )
        (self.root / "tests").mkdir()
        tabula.update(self.root)
        state = json.loads((self.root / ".tabula/state.json").read_text())
        self.assertEqual(list(state["sources"]), ["source file.py"])

    def test_path_escapes_credentials_state_and_symlinks_are_rejected(self):
        self.init()
        (self.root / "link.py").symlink_to(self.root / "README.md")
        (self.root / "linked").symlink_to(self.root, target_is_directory=True)
        for target in (
            "../../outside.py",
            "../%2e%2e/outside.py",
            "/etc/passwd",
            "../.env",
            "../.ssh/id_rsa",
            "../secret.pem",
            "state.json",
            "../link.py",
            "../linked/README.md",
            "../README.md?raw=1",
        ):
            with self.subTest(target=target):
                (self.root / ".tabula/ENTRY.md").write_text(
                    f"# Fixture\n[x]({target})\n"
                )
                with self.assertRaises(ValueError):
                    tabula.update(self.root)

    def test_special_and_large_files_are_bounded(self):
        self.init()
        path = self.root / "source.py"
        entry = self.root / ".tabula/ENTRY.md"
        entry.write_text("# Fixture\n[source](../source.py)\n")
        os.mkfifo(path)
        with self.assertRaises(ValueError):
            tabula.update(self.root)
        path.unlink()
        path.write_bytes(b"x" * (tabula.SOURCE_LIMIT + 1))
        with self.assertRaises(ValueError):
            tabula.update(self.root)
        entry.write_bytes(b"# " + b"x" * tabula.ENTRY_LIMIT)
        with self.assertRaises(ValueError):
            tabula.update(self.root)

    def test_state_symlink_cannot_redirect_writes(self):
        self.init()
        state = self.root / ".tabula/state.json"
        state.unlink()
        state.symlink_to(self.root / "README.md")
        original = (self.root / "README.md").read_bytes()
        with self.assertRaises(ValueError):
            tabula.update(self.root)
        self.assertEqual((self.root / "README.md").read_bytes(), original)

    def test_cli_status_is_machine_readable_and_does_not_execute_repo_code(self):
        # Executable declarations are evidence, never commands to source.
        (self.root / ".envrc").write_text("touch SHOULD_NOT_EXIST\n")
        command = [
            sys.executable,
            str(Path(tabula.__file__)),
            "init",
            str(self.root),
            "--purpose",
            "Fixture",
            "--json",
        ]
        result = subprocess.run(command, text=True, capture_output=True, check=True)
        self.assertEqual(json.loads(result.stdout)["status"], "created")
        self.assertFalse((self.root / "SHOULD_NOT_EXIST").exists())
        (self.root / "README.md").write_text("changed\n")
        result = subprocess.run(
            command[:2] + ["check", str(self.root), "--json"],
            check=False,
            text=True,
            capture_output=True,
        )
        self.assertEqual(result.returncode, 1)
        self.assertEqual(json.loads(result.stdout)["status"], "stale")

    def test_malformed_receipt_returns_error_without_echoing_content(self):
        self.init()
        (self.root / ".tabula/state.json").write_text('{"PRIVATE":')
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            status = tabula.main(["check", str(self.root), "--json"])
        self.assertEqual(status, 2)
        self.assertNotIn("PRIVATE", output.getvalue())


if __name__ == "__main__":
    unittest.main()
