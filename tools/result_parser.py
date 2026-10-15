"""Parse test counts from supported test-runner summaries."""

import re
from dataclasses import dataclass


@dataclass(frozen=True)
class TestCounts:
    passed: int
    failed: int
    skipped: int
    filtered_out: int = 0


def parse_rust_summary(output: str) -> TestCounts | None:
    """Return counts for exactly one Rust test summary, otherwise None."""
    summaries = [
        line.strip()
        for line in output.splitlines()
        if line.strip().startswith("test result:")
    ]

    if len(summaries) != 1:
        return None

    match = re.fullmatch(
        r"test result: (ok|FAILED)\. "
        r"(\d+) passed; (\d+) failed; (\d+) ignored; "
        r"(\d+) measured; (\d+) filtered out; "
        r"finished in [0-9.]+s",
        summaries[0],
    )

    if match is None:
        return None

    status = match.group(1)
    passed, failed, skipped, measured, filtered_out = (
        int(value) for value in match.groups()[1:]
    )

    # Benchmarks are outside this parser's supported test format.
    if measured != 0:
        return None

    if (status == "ok") != (failed == 0):
        return None

    return TestCounts(
        passed=passed,
        failed=failed,
        skipped=skipped,
        filtered_out=filtered_out,
    )


def parse_python_summary(output: str) -> TestCounts | None:
    """Return counts for exactly one unittest summary, otherwise None."""
    lines = [line.strip() for line in output.splitlines()]

    runs = []
    statuses = []

    for index, line in enumerate(lines):
        run = re.fullmatch(
            r"Ran (\d+) tests? in [0-9.]+s",
            line,
        )
        if run is not None:
            runs.append((index, int(run.group(1))))

        status = re.fullmatch(
            r"(OK|FAILED)(?: \(([^()]*)\))?",
            line,
        )
        if status is not None:
            statuses.append((index, status))

    if len(runs) != 1 or len(statuses) != 1:
        return None

    run_index, total = runs[0]
    status_index, status = statuses[0]

    if status_index <= run_index:
        return None

    counts = {
        "failures": 0,
        "errors": 0,
        "skipped": 0,
    }
    seen = set()
    details = status.group(2)

    if details is not None:
        for entry in details.split(","):
            match = re.fullmatch(
                r"(failures|errors|skipped)=(\d+)",
                entry.strip(),
            )

            if match is None:
                return None

            key, value = match.groups()

            if key in seen:
                return None

            seen.add(key)
            counts[key] = int(value)

    failed = counts["failures"] + counts["errors"]
    skipped = counts["skipped"]
    passed = total - failed - skipped

    if passed < 0:
        return None

    if (status.group(1) == "OK") != (failed == 0):
        return None

    return TestCounts(
        passed=passed,
        failed=failed,
        skipped=skipped,
    )
