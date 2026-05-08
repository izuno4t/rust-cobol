#!/usr/bin/env python3

from __future__ import annotations

import re
import sys
from collections import defaultdict
from pathlib import Path


def classify(log_text: str) -> str:
    if "_suppress_debug_event" in log_text:
        return "debug-declarative helper undeclared in generated C"
    if re.search(r"use of undeclared identifier '.*__(?:HOURS|HRS)'", log_text):
        return "communication symbolic sub-queue/time field lowering bug"
    if re.search(r"use of undeclared identifier '.*__FILLER'", log_text):
        return "CORRESPONDING/group field codegen emits filler symbol"
    if "macro redefined" in log_text:
        return "generated macro name collision"

    match = re.search(r"error:\s+(.+)", log_text)
    if match:
        return f"other compiler error: {match.group(1).strip()}"
    return "unknown"


def main() -> int:
    env_root = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(".nist")
    results_root = env_root / "results"
    if not results_root.exists():
        print(f"Results not found: {results_root}", file=sys.stderr)
        return 1

    grouped: dict[str, list[str]] = defaultdict(list)
    for status_file in sorted(results_root.rglob("*.status")):
        status = status_file.read_text().strip()
        if status != "COMPILE_ERROR":
            continue
        compile_log = status_file.with_suffix(".compile.log")
        log_text = compile_log.read_text(errors="ignore") if compile_log.exists() else ""
        grouped[classify(log_text)].append(f"{status_file.parent.name}/{status_file.stem}")

    total = sum(len(programs) for programs in grouped.values())
    print("=== NIST Compile Error Classes ===")
    print(f"Total: {total}")
    for cls, programs in sorted(grouped.items(), key=lambda item: (-len(item[1]), item[0])):
        print(f"[{len(programs)}] {cls}")
        print("  " + ", ".join(programs))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
