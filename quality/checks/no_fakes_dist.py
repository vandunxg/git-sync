#!/usr/bin/env python3
"""TEST-NO-FAKES (build level): inspect the built renderer bundle for test/demo content.

Run after `pnpm --dir apps/desktop build`. Exit 0 PASS, 1 FAIL, 2 BLOCKED (no bundle).
Markers are strings that only test fixtures, demo scaffolds or debug tooling contain.
This is a heuristic guard: identifiers can be renamed by the minifier, so string
literals (case labels, event names, copy) are the reliable markers.
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DIST = ROOT / "apps/desktop/dist"
MARKERS = {
    "shared contract cases": ["maxLength-astral", "pattern-insert:", "wrong-type:"],
    "throwaway probes": ["PROBE ok="],
    "template debug panel": ["DebugPanel", "ctui:debug", "trackedInvoke"],
    "template demo": ["Project ready!", "greet", "dashboard-01"],
    "remote code": ["http://", "https://cdn", "https://fonts.googleapis"],
}
# `http://` appears in third-party library code (e.g. XML namespaces in React DOM);
# only flag it inside the HTML entry, where it would mean a remote script/style.
HTML_ONLY = {"remote code"}


def main() -> int:
    files = sorted(p for p in DIST.rglob("*") if p.is_file() and p.suffix in {".js", ".mjs", ".css", ".html", ".json", ".svg", ".txt", ".map"})
    if not files:
        print(json.dumps({"status": "BLOCKED", "reason": f"no built bundle under {DIST.relative_to(ROOT)}"}))
        return 2
    hits = []
    for path in files:
        text = path.read_text(encoding="utf-8", errors="replace")
        for group, markers in MARKERS.items():
            if group in HTML_ONLY and path.suffix != ".html":
                continue
            hits += [{"file": str(path.relative_to(ROOT)), "group": group, "marker": m} for m in markers if m in text]
    report = {"status": "FAIL" if hits else "PASS", "filesScanned": len(files), "hits": hits}
    print(json.dumps(report, indent=2))
    return 1 if hits else 0


if __name__ == "__main__":
    sys.exit(main())
