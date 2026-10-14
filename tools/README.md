# Development tools

Run the shared task runner from the repository root using Python 3.9 or newer. It uses only the Python standard library; no virtual environment or pip packages are required for this script.

| Command | Task |
| --- | --- |
| `python tools/dev.py test` | Run both signal and control library host unit tests with lockfiles enforced |
| `python tools/dev.py fmt` | Format all three Rust packages; modifies source files |
| `python tools/dev.py check` | Check formatting of all three packages, run both libraries' host tests, and build the Pico firmware |
| `python tools/dev.py uf2` | Build and convert the Pico firmware using the configured Cargo runner |

On Ubuntu, use python3 if python is unavailable. Rust/Cargo must be on PATH. The check task requires rustfmt and the thumbv6m-none-eabi target. The uf2 task additionally requires the installed elf2uf2-rs converter.

The runner resolves package directories relative to its own file and starts subprocesses without a shell. It stops at the first failing command and returns its exit code. It does not install prerequisites or flash hardware.

A typical local workflow is:

```bash
python tools/dev.py check
```

If formatting fails:

```bash
python tools/dev.py fmt
git diff
python tools/dev.py check
```

The owner reported a successful expanded local check on Windows on 2026-10-14. This covers formatting of three packages, six signal tests, four Angle tests, and the existing Pico build. The control library is not yet integrated into the Pico firmware. No Python-specific automated tests have been introduced. The previous GitHub Actions workflow was reported green for Windows/Ubuntu tests and the Ubuntu firmware build; the new Angle tests await their first reported remote run.

The [GitHub Actions workflow](../.github/workflows/rust.yml) uses the test task for the Windows/Ubuntu matrix and the check task for Ubuntu validation. Both jobs provision Python 3.13 and Rust 1.99.0 before invoking the shared runner. The Ubuntu check job additionally installs rustfmt and the Pico target. The previous workflow run was reported successful; execution with the expanded Angle test task remains pending.

See [development setup](../docs/development-setup.md) and [firmware instructions](../firmware/README.md) for installation, manual Cargo commands, and hardware verification limits.
