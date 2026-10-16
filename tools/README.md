# Development tools

Use Python 3.10 or newer; no external Python packages are needed. From the repository root:

| Command | Task |
| --- | --- |
| `python tools/dev.py test` | Python runner/parser tests and host tests for servo-signal, servo-control, and servo-pio |
| `python tools/dev.py fmt` | Format all four Rust packages; modifies source files |
| `python tools/dev.py check` | Python tests, four Rust formatting checks, three Rust test groups, Pico build |
| `python tools/dev.py uf2` | Build and convert firmware with the Cargo runner; no automatic upload |

On Ubuntu use python3 if python is unavailable. Cargo must be on PATH. check requires rustfmt and thumbv6m-none-eabi. uf2 additionally requires elf2uf2-rs.

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

Python tests mock process execution, so they do not launch Cargo recursively. The runner's test/check tasks discover both test_dev.py and test_result_parser.py. Coverage includes command capture, failure handling, aggregate process status, test totals, unknown summaries, and parser fixtures.

The owner previously reported the 15 parser tests passing and successful local/CI runs before this integration. The new runner integration and added tests have not been executed by the assistant; local and remote verification are pending.

## Workflow

```bash
python tools/dev.py check
```

If formatting fails, run fmt, review git diff, then rerun check. GitHub Actions calls the same test/check tasks on its configured Windows/Ubuntu runners. No workflow modification is needed for the new test discovery.

See [development setup](../docs/development-setup.md) and [firmware instructions](../firmware/README.md).

The runner also colors its own step results and overall status: only PASS/PASSED is green and FAIL/FAILED is red; brackets, command text, counts, and headers stay plain. NO_COLOR and plain redirected output disable these colors. Numeric totals remain plain text.

The servo-pio encoder package is included in test, fmt, and check. The owner reported 102 passing tests in the full local check after adding its nine encoding tests. CI uses the same task definitions; the workflow file requires no change for this package. The check task still includes a firmware build; the proposed task naming changes remain deferred.
