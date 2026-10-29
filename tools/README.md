# Development tools

Use Python 3.10 or newer; no external Python packages are needed. From the repository root:

| Command | Task |
| --- | --- |
| `python tools/dev.py install` | Install Rust components, the Pico target and elf2uf2-rs for the active toolchain |
| `python tools/dev.py doc` | Generate combined HTML documentation for all six Rust packages |
| `python tools/dev.py test` | Python runner/parser tests and host tests for servo-signal, servo-control, and servo-pio |
| `python tools/dev.py fmt` | Format all four Rust packages; modifies source files |
| `python tools/dev.py check` | Check formatting and run cargo check for all four Rust packages; no unit tests or linked firmware image |
| `python tools/dev.py build` | Build the Pico ELF firmware; no UF2 conversion |
| `python tools/dev.py verify` | Full verification: Python tests, four Rust formatting checks, three Rust test groups, and Pico build |
| `python tools/dev.py hardware-build` | Build all registered Pico hardware tests with ELF, UF2, linker map and disassembly; no execution or upload |
| `python tools/dev.py uf2` | Build and convert firmware with the Cargo runner; no automatic upload |

On Ubuntu use python3 if python is unavailable. Cargo must be on PATH. check and verify require rustfmt and thumbv6m-none-eabi. build requires the embedded target. uf2 additionally requires elf2uf2-rs.

## Output and totals

The runner resolves directories from its own path and launches commands without a shell. It displays merged stdout/stderr line by line and captures the same text for summary parsing. Child Python output uses UTF-8 and unbuffered mode; child colors are enabled for interactive terminals and GitHub Actions. NO_COLOR explicitly disables colors; redirected non-CI output stays plain. Rust test-harness colors are explicitly enabled when appropriate. Captured text retains ANSI colors; only the parser input is stripped of ANSI control sequences.

All independent task steps run even after failures. The final overview lists each step and its process status. A nonzero child exit code causes a nonzero overall exit code, regardless of later successes or parsed test counts. Missing/unlaunchable commands are reported as failed steps.

Only Cargo test commands and Python unittest commands contribute test counts. Formatting, builds, and UF2 conversion do not. The total sums passed, failed, skipped/ignored, and filtered-out cases separately; filtered-out cases were not executed.

Each test process must contain exactly one recognizable summary. Compilation errors, unsupported categories, and ambiguous/missing summaries produce "Test count: unknown". If any group is unknown, only a "Known subtotal" is shown, explicitly marked incomplete. An unknown count alone does not override a successful process exit code.

The parser handles the currently used Rust libtest and Python unittest text formats. It does not use unstable Rust JSON output. New output formats require parser updates and tests. Benchmarks and unittest expected-failure categories are not supported. Counts are never hard-coded.

The uf2 task uses cargo run: conversion remains dependent on a successful build inside Cargo. The runner itself does not install tools or flash hardware.

## Tests

```bash
python -m unittest discover -s tools -p "test_*.py" -v
```

Python tests mock process execution, so they do not launch Cargo recursively. The runner's test/verify tasks discover both test_dev.py and test_result_parser.py. Coverage includes command capture, failure handling, aggregate process status, test totals, unknown summaries, and parser fixtures.

The owner previously reported the 15 parser tests passing and successful local/CI runs before this integration. The new runner integration and added tests have not been executed by the assistant; local and remote verification are pending.

## Workflow

```bash
python tools/dev.py verify
```

If formatting fails, run fmt, review git diff, then rerun verify. GitHub Actions calls the same test/verify tasks on its configured Windows/Ubuntu runners. No workflow modification is needed for the new test discovery.

See [development setup](../docs/development-setup.md) and [firmware instructions](../firmware/README.md).

The runner also colors its own step results and overall status: only PASS/PASSED is green and FAIL/FAILED is red; brackets, command text, counts, and headers stay plain. NO_COLOR and plain redirected output disable these colors. Numeric totals remain plain text.

The servo-pio encoder package is included in test, fmt, check, and verify. The owner reported 102 passing tests in the full local check after adding its nine encoding tests. CI uses the same task definitions; the workflow file requires no change for this package. The previous full check task is now named verify. check only checks Rust formatting and compilation; it does not run tests or link the Pico firmware. Cargo may still compile dependencies and execute build scripts as required by cargo check.

## Hardware-test builds

From the repository root:

```bash
python tools/dev.py hardware-build
```

This runs `cargo build --locked --examples --features hardware-tests --message-format=json-render-diagnostics` in firmware/pico. Cargo discovers the registered example targets in Cargo.toml, including those stored under hardware_tests. Adding another registered hardware test does not require listing its name in dev.py.

The task produces ELF executables under firmware/pico/target/thumbv6m-none-eabi/debug/examples. It requires the Pico Rust target, elf2uf2-rs and llvm-tools-preview in the active Rust toolchain. It does not execute tests or add them to unit-test totals. Hardware-test programs are not built or executed by test, check, or verify; hardware-build remains explicit.

To build and convert one selected hardware test to UF2, run from firmware/pico:

```bash
cargo run --locked --example underflow_missing_counter --features hardware-tests
```

The configured Cargo runner performs conversion only; transfer to the Pico and evaluation remain manual. hardware-build reads Cargo compiler-artifact messages and invokes elf2uf2-rs directly for each reported example executable, including unchanged/fresh artifacts. UF2 files are written beside their ELF files. A failed build skips all conversions; a conversion failure keeps the task failed even if later conversions succeed. No executable artifacts after a successful build are treated as an error. Existing UF2 files can remain after a failed run; use only results from a fully successful run.

The owner reported a full local check with 121 passing unit tests and successful creation of the hardware-test UF2 files. Seven runner tests cover Cargo artifact discovery, cached artifacts, build-failure conversion suppression, per-artifact conversion, conversion failure propagation, missing artifacts, and diagnostic display. See the [manual hardware-test guide](../firmware/pico/hardware_tests/README.md) for setup and expected signals. Remote CI for this revision is pending.

## Linker maps and disassembly

Install the additional Rust component once (Windows and Ubuntu):

```bash
rustup component add llvm-tools-preview
```

`python tools/dev.py hardware-build` now produces `<example>.map` and `<example>.disasm` beside each ELF and UF2 in the target examples directory. The map records linker placement; the disassembly contains section headers, symbols and disassembled executable sections, with demangled names. Generated files remain under ignored target directories. These files support manual review; their generation does not automatically validate RAM-only execution.

After the initial all-example build discovers Cargo artifacts, the runner invokes `cargo rustc` once per example with a separate linker map destination. Each uses a temporary unique map path to force relinking, including when previous reports have been removed. Dependencies still use Cargo's cache. LLVM is found under the active `rustc --print sysroot`; cargo-binutils is not required. Disassembly is written directly to a temporary file, not printed to the terminal. After successful generation, reports replace the previous files, then UF2 conversion uses the newly linked ELF.

Missing LLVM tools, failed relinking, unexpected ELF output, missing/empty maps or disassembly, and conversion errors fail the overall task. A failed report step skips UF2 conversion for that example; other examples are still processed. Existing outputs may remain after failure and must not be treated as current. Only use a fully successful run. Normal build/check/verify tasks retain their existing behavior and do not require LLVM tools. Automated tests mock Cargo and LLVM; the owner runs verification and real hardware builds.

## Install development tools

After the [Windows or Ubuntu prerequisites](../docs/development-setup.md) are available, run:

```bash
python tools/dev.py install
```

The command installs rustfmt, clippy and llvm-tools-preview for the active Rust toolchain, the thumbv6m-none-eabi target, and elf2uf2-rs version 2.2.0 with its packaged lockfile. Commands run from firmware/pico to respect the same rustup overrides as hardware builds. The converter explicitly targets the host architecture, not the Pico. Existing matching installations can be reused without forcing reinstallation. It does not install or update Rust itself or change the default toolchain.

Python 3.10+, Rustup with a working Rust/Cargo toolchain, and host build prerequisites must already be installed and available on PATH. Windows requires the MSVC C++ build tools and Windows SDK; Ubuntu requires the documented build-essential setup. Run as your normal user; the task does not invoke sudo or an OS package manager, modify PATH, install editor extensions, or flash hardware. Downloads require network access. Cargo's bin directory must be on PATH for later UF2 commands.

All three installation steps appear in the summary. Independent steps are attempted even after a failure, and any failure makes the overall task fail. Installation does not count as unit tests. The new tests mock all commands; actual installation and verification are performed by the owner.

## Combined Rust documentation

Run from the repository root:

```bash
python tools/dev.py doc
```

The task documents servo-signal, servo-control, servo-pio, logging, servo-storage and the Pico binary using cargo doc --locked --no-deps --document-private-items. All packages use the thumbv6m-none-eabi target, so embedded-only PIO code is included. Cargo/Rustdoc and the embedded target are required; no extra Python package, cargo-modules or Graphviz installation is needed.

After all six commands succeed, an overview links the generated crate pages at:

```text
target/rustdoc/thumbv6m-none-eabi/doc/index.html
```

Open this file manually in a browser. The task does not automatically open windows. The shared output contains the project's package documentation, with module navigation, types, trait implementations and implementors. Dependency documentation is omitted; some external links therefore lead outside the generated pages. This is navigable API documentation, not an automatically generated architecture graph. Only compiled library/binary modules for the selected target and default features are included; hardware-test examples and modules used only by those examples are excluded.

Commands run sequentially in their package directories with a shared target directory. Rustdoc output is cumulative. A failed package does not prevent the remaining commands, but suppresses overview generation and fails the task. Missing expected crate pages also fail overview generation. Old generated files may remain after a failure: use only a successful run and regenerate after changes. Generated output is ignored by Git. The task does not execute doctests, host tests, hardware programs or firmware uploads and is separate from verify. Tests mock Cargo; real generation and browser review remain with the owner.
