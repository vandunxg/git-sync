#!/usr/bin/env python3
"""Validate baseline file records, not visual quality or human authenticity."""
from __future__ import annotations
import argparse
import hashlib
import json
import re
import struct
from datetime import datetime
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]


def contained_file(root: Path, relative: Any) -> Path:
    if not isinstance(relative, str) or not relative: raise ValueError('missing relative path')
    p = Path(relative)
    if p.is_absolute() or '..' in p.parts: raise ValueError('path escapes the blueprint')
    full = (root / p).resolve()
    if not full.is_relative_to(root.resolve()): raise ValueError('symlink escapes the blueprint')
    if not full.is_file(): raise ValueError('file does not exist')
    return full


def png_dimensions(path: Path) -> tuple[int, int]:
    with path.open('rb') as fh: head = fh.read(33)
    if len(head) < 33 or head[:8] != b'\x89PNG\r\n\x1a\n' or head[12:16] != b'IHDR':
        raise ValueError('not a PNG with IHDR')
    if struct.unpack('>I', head[8:12])[0] != 13: raise ValueError('invalid IHDR size')
    width, height = struct.unpack('>II', head[16:24])
    if not width or not height: raise ValueError('zero image dimension')
    return width, height


def audit(root: Path = ROOT, require_goldens: bool = False) -> dict[str, Any]:
    reasons: list[str] = []
    try:
        registry = json.loads((root / 'docs/ui/BASELINES.json').read_text(encoding='utf-8'))
        screens = json.loads((root / 'docs/ui/SCREENS.json').read_text(encoding='utf-8'))['screens']
    except (OSError, ValueError, KeyError) as exc:
        return {'status': 'BLOCKED', 'reasons': [f'cannot load baseline registries: {exc}']}
    entries = registry.get('screens', [])
    by_id = {e.get('screenId'): e for e in entries}
    if len(entries) != len(by_id): reasons.append('duplicate baseline screen ID')
    for screen in screens:
        if not screen.get('baselineRequired'): continue
        sid = screen['id']; entry = by_id.get(sid)
        if entry is None: reasons.append(f'{sid}: missing baseline record'); continue
        if entry.get('status') != 'approved': reasons.append(f'{sid}: baseline not owner approved')
        approval = entry.get('approval', {})
        for k in ('by', 'at', 'recordPath'):
            if not approval.get(k): reasons.append(f'{sid}: missing approval.{k}')
        if approval.get('at'):
            try:
                if datetime.fromisoformat(approval['at'].replace('Z', '+00:00')).tzinfo is None: raise ValueError('timezone')
            except (ValueError, TypeError): reasons.append(f'{sid}: invalid approval timestamp')
        if approval.get('recordPath'):
            try:
                if contained_file(root, approval['recordPath']).stat().st_size == 0: raise ValueError('empty record')
            except ValueError as exc: reasons.append(f'{sid}: approval evidence {exc}')
        sources = ['designReference'] + (['implementationGolden'] if require_goldens else [])
        resolved: list[Path] = []
        for source in sources:
            artifact = entry.get(source, {})
            try:
                p = contained_file(root, artifact.get('path')); resolved.append(p)
                digest = artifact.get('sha256')
                if not isinstance(digest, str) or not re.fullmatch(r'[a-f0-9]{64}', digest): raise ValueError('missing/invalid SHA-256')
                if hashlib.sha256(p.read_bytes()).hexdigest() != digest: raise ValueError('SHA-256 mismatch')
                vp = entry.get('viewport', {})
                if png_dimensions(p) != (vp.get('width'), vp.get('height')): raise ValueError('viewport/image dimensions mismatch')
                if source == 'implementationGolden':
                    if not artifact.get('commit') or not artifact.get('renderer'): raise ValueError('missing implementation commit/renderer')
            except (OSError, ValueError) as exc: reasons.append(f'{sid}: {source}: {exc}')
        if len(resolved) == 2 and resolved[0] == resolved[1]: reasons.append(f'{sid}: design and implementation paths must differ')
    return {'status': 'BLOCKED' if reasons else 'PASS_RECORD_INTEGRITY_ONLY', 'reasons': reasons,
            'screensChecked': len(screens), 'visualApprovalProven': False,
            'note': 'File integrity is not visual review or proof of human consent. Subviews/native review remain required.'}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=ROOT)
    parser.add_argument('--require-goldens', action='store_true')
    args = parser.parse_args()
    result = audit(args.root.resolve(), args.require_goldens)
    print(json.dumps(result, indent=2))
    return 2 if result['status'] == 'BLOCKED' else 0

if __name__ == '__main__': raise SystemExit(main())
