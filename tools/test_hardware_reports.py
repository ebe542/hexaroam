"""Map/disassembly generation tests; no Rust tools are executed."""

import io
import json
import subprocess
import tempfile
import unittest
from contextlib import redirect_stdout
from pathlib import Path
from unittest.mock import patch

import dev


class HardwareReportTests(unittest.TestCase):
    def setUp(self):
        temporary = self.enterContext(tempfile.TemporaryDirectory())
        self.directory = Path(temporary) / "pico project"
        self.directory.mkdir()
        self.executable = self.directory / "target" / "examples" / "probe"
        self.executable.parent.mkdir(parents=True)
        self.executable.write_bytes(b"ELF fixture")
        self.objdump = Path(temporary) / "llvm tools" / "llvm-objdump"
        self.enterContext(redirect_stdout(io.StringIO()))

    def link(self, arguments, directory):
        self.assertEqual(directory, self.directory)
        self.assertEqual(arguments[:6], [
            "cargo", "rustc", "--locked", "--example", "probe", "--features",
        ])
        self.assertIn("hardware-tests", arguments)
        map_path = Path(arguments[-1].removeprefix("link-arg=-Map="))
        map_path.write_text("map fixture", encoding="utf-8")
        return dev.CommandResult(0, json.dumps({
            "reason": "compiler-artifact", "target": {"kind": ["example"]},
            "executable": str(self.executable),
        }) + "\n")

    def disassemble(self, arguments, **options):
        self.assertEqual(arguments[:-1], [
            str(self.objdump), "--disassemble", "--demangle",
            "--section-headers", "--syms",
        ])
        # Windows can expand a temporary directory's 8.3 alias to its
        # long name during resolve(). Compare canonical paths.
        self.assertEqual(Path(arguments[-1]).resolve(), self.executable.resolve())
        self.assertEqual(options["cwd"], self.directory)
        self.assertFalse(options["shell"])
        self.assertEqual(options["stderr"], subprocess.PIPE)
        options["stdout"].write("disassembly fixture\n")
        return subprocess.CompletedProcess(arguments, 0, stderr="")

    def generate(self, link=None, dump=None):
        with (
            patch.object(dev, "find_objdump", return_value=self.objdump),
            patch.object(dev, "run_command", side_effect=link or self.link),
            patch.object(dev.subprocess, "run", side_effect=dump or self.disassemble),
        ):
            return dev.hardware_reports(self.executable, self.directory)

    def test_success_writes_reports_beside_elf_with_spaces_in_paths(self):
        result = self.generate()
        self.assertEqual(result.return_code, 0)
        self.assertEqual(self.executable.with_suffix(".map").read_text(), "map fixture")
        self.assertEqual(self.executable.with_suffix(".disasm").read_text(),
                         "disassembly fixture\n")
        self.assertEqual(list(self.executable.parent.glob(".reports-*")), [])

    def test_cached_link_is_avoided_by_unique_map_destination(self):
        destinations = []
        def link(arguments, directory):
            destinations.append(arguments[-1])
            return self.link(arguments, directory)
        self.assertEqual(self.generate(link=link).return_code, 0)
        self.assertEqual(self.generate(link=link).return_code, 0)
        self.assertNotEqual(destinations[0], destinations[1])

    def test_failed_link_skips_disassembly(self):
        with (
            patch.object(dev, "find_objdump", return_value=self.objdump),
            patch.object(dev, "run_command", return_value=dev.CommandResult(7, "failed")),
            patch.object(dev.subprocess, "run") as dump,
        ):
            result = dev.hardware_reports(self.executable, self.directory)
        self.assertEqual(result.return_code, 7)
        dump.assert_not_called()

    def test_missing_map_is_failure(self):
        def link(arguments, directory):
            result = self.link(arguments, directory)
            Path(arguments[-1].removeprefix("link-arg=-Map=")).unlink()
            return result
        self.assertNotEqual(self.generate(link=link).return_code, 0)

    def test_unexpected_elf_is_failure(self):
        def link(arguments, directory):
            return dev.CommandResult(0, self.link(arguments, directory).output.replace(
                'probe"', 'other"',
            ))
        self.assertNotEqual(self.generate(link=link).return_code, 0)

    def test_failed_dump_preserves_previous_reports(self):
        self.executable.with_suffix(".map").write_text("previous map")
        self.executable.with_suffix(".disasm").write_text("previous dump")
        def dump(arguments, **options):
            options["stdout"].write("partial output")
            return subprocess.CompletedProcess(arguments, 9, stderr="dump failed")
        self.assertEqual(self.generate(dump=dump).return_code, 9)
        self.assertEqual(self.executable.with_suffix(".map").read_text(), "previous map")
        self.assertEqual(self.executable.with_suffix(".disasm").read_text(), "previous dump")

    def test_empty_dump_is_failure(self):
        def dump(arguments, **options):
            return subprocess.CompletedProcess(arguments, 0, stderr="")
        self.assertNotEqual(self.generate(dump=dump).return_code, 0)

    def test_missing_tool_reports_install_command(self):
        with patch.object(dev, "find_objdump", side_effect=ValueError(
            "Run: rustup component add llvm-tools-preview",
        )):
            result = dev.hardware_reports(self.executable, self.directory)
        self.assertNotEqual(result.return_code, 0)
        self.assertIn("rustup component add llvm-tools-preview", result.output)

    def test_find_objdump_in_active_sysroot_for_windows_and_linux_names(self):
        for filename in ("llvm-objdump.exe", "llvm-objdump"):
            with self.subTest(filename=filename), tempfile.TemporaryDirectory() as temporary:
                tool = Path(temporary) / "lib" / "rustlib" / "host" / "bin" / filename
                tool.parent.mkdir(parents=True)
                tool.write_bytes(b"fixture")
                with patch.object(dev.subprocess, "run", return_value=
                                  subprocess.CompletedProcess([], 0, temporary + "\n", "")) as run:
                    self.assertEqual(dev.find_objdump(self.directory), tool)
                self.assertEqual(run.call_args.args[0], ["rustc", "--print", "sysroot"])
                self.assertEqual(run.call_args.kwargs["cwd"], self.directory)

    def test_failed_sysroot_lookup_is_not_hidden(self):
        with patch.object(dev.subprocess, "run", return_value=
                          subprocess.CompletedProcess([], 1, "", "rustc failed")):
            with self.assertRaisesRegex(ValueError, "rustc failed"):
                dev.find_objdump(self.directory)

    def test_report_failure_skips_only_its_conversion_and_remains_failure(self):
        artifacts = "".join(json.dumps({
            "reason": "compiler-artifact", "target": {"kind": ["example"]},
            "executable": path,
        }) + "\n" for path in ("out/first", "out/second"))
        with (
            patch.object(dev, "TASKS", {"hardware-build": [(self.directory, ["cargo", "build"])]}),
            patch.object(dev, "run_command", side_effect=[
                dev.CommandResult(0, artifacts), dev.CommandResult(0, "converted"),
            ]) as run,
            patch.object(dev, "hardware_reports", side_effect=[
                dev.CommandResult(1, "reports failed"), dev.CommandResult(0, ""),
            ]),
        ):
            code = dev.run_task("hardware-build")
        self.assertNotEqual(code, 0)
        self.assertEqual(run.call_count, 2)
        self.assertIn(str(Path("out/second")), run.call_args.args[0])


if __name__ == "__main__":
    unittest.main()
