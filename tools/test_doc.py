"""Tests for combined Rust documentation; Cargo is never executed."""

import io
import tempfile
import unittest
from contextlib import redirect_stdout
from pathlib import Path
from unittest.mock import patch

import dev


class DocumentationTests(unittest.TestCase):
    def test_documents_all_packages_with_shared_embedded_output(self):
        self.assertEqual({directory for directory, _ in dev.TASKS["doc"]}, {
            dev.SIGNAL_DIR, dev.CONTROL_DIR, dev.SERVO_PIO_DIR,
            dev.LOGGING_DIR, dev.STORAGE_DIR, dev.PROTOCOL_DIR,
            dev.REGISTERS_DIR, dev.PICO_DIR,
        })
        self.assertEqual(len(dev.TASKS["doc"]), 8)
        for directory, command in dev.TASKS["doc"]:
            self.assertEqual(command[:2], ["cargo", "doc"])
            for flag in ("--locked", "--no-deps", "--document-private-items"):
                self.assertIn(flag, command)
            self.assertEqual(command[command.index("--target") + 1], "thumbv6m-none-eabi")
            self.assertEqual(command[command.index("--target-dir") + 1], str(dev.DOC_TARGET_DIR))
            self.assertNotIn("--open", command)
            if directory == dev.PICO_DIR:
                self.assertEqual(command[-2:], ["--bin", "hexaroam-pico"])
            else:
                self.assertEqual(command[-1], "--lib")

    def test_success_creates_index_without_test_totals(self):
        with (
            patch.object(dev, "run_command", return_value=dev.CommandResult(0, "")) as run,
            patch.object(dev, "write_doc_index", return_value=dev.CommandResult(0, "")) as index,
            redirect_stdout(io.StringIO()) as output,
        ):
            code = dev.run_task("doc")
        self.assertEqual(code, 0)
        self.assertEqual(run.call_count, len(dev.TASKS["doc"]))
        index.assert_called_once_with()
        self.assertIn(f"0 of {len(dev.TASKS['doc']) + 1} checks failed", output.getvalue())
        self.assertNotIn("\nTotal:", output.getvalue())

    def test_failed_package_skips_index_but_processes_remaining_packages(self):
        with (
            patch.object(dev, "run_command", side_effect=[
                dev.CommandResult(1, "failed"),
                *[dev.CommandResult(0, "") for _ in range(len(dev.TASKS["doc"]) - 1)],
            ]) as run,
            patch.object(dev, "write_doc_index") as index,
            redirect_stdout(io.StringIO()),
        ):
            self.assertNotEqual(dev.run_task("doc"), 0)
        self.assertEqual(run.call_count, len(dev.TASKS["doc"]))
        index.assert_not_called()

    def test_index_failure_fails_task(self):
        with (
            patch.object(dev, "run_command", return_value=dev.CommandResult(0, "")),
            patch.object(dev, "write_doc_index", return_value=dev.CommandResult(1, "failed")),
            redirect_stdout(io.StringIO()),
        ):
            self.assertNotEqual(dev.run_task("doc"), 0)

    def test_index_contains_relative_links_to_all_package_pages(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)
            for _, name in dev.DOC_PACKAGES:
                page = output / name.replace("-", "_") / "index.html"
                page.parent.mkdir()
                page.write_text("fixture", encoding="utf-8")
            with patch.object(dev, "DOC_OUTPUT_DIR", output), redirect_stdout(io.StringIO()):
                result = dev.write_doc_index()
            self.assertEqual(result.return_code, 0)
            text = (output / "index.html").read_text(encoding="utf-8")
            for _, name in dev.DOC_PACKAGES:
                self.assertIn(f'href="{name.replace("-", "_")}/index.html"', text)
            self.assertNotIn(str(output), text)

    def test_missing_package_page_does_not_publish_overview(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)
            with patch.object(dev, "DOC_OUTPUT_DIR", output), redirect_stdout(io.StringIO()):
                result = dev.write_doc_index()
            self.assertNotEqual(result.return_code, 0)
            self.assertIn("Missing generated documentation", result.output)
            self.assertFalse((output / "index.html").exists())


if __name__ == "__main__":
    unittest.main()
