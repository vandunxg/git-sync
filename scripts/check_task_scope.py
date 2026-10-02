#!/usr/bin/env python3
"""Check tracked and untracked changed paths against one task's declared allowlist.

This checks path scope only, not semantics, approvals or requirement correctness.
"""
from __future__ import annotations
import argparse
import json
import subprocess
from pathlib import Path, PurePosixPath
from typing import Any

ROOT=Path(__file__).resolve().parents[1]
COMMON=['state/IMPLEMENTATION.json','state/evidence/','state/locks/']


def allowed(path: str, patterns: list[str]) -> bool:
    p=PurePosixPath(path)
    if p.is_absolute() or '..' in p.parts or not path: return False
    return any(path.startswith(pattern) if pattern.endswith('/') else path==pattern for pattern in patterns)


def check_paths(paths: list[str], task: dict[str,Any], blueprint_relative: str | None = None) -> list[str]:
    patterns=task['allowedPaths']+COMMON
    bad=[]
    for path in paths:
        candidates=[path]
        if blueprint_relative and path.startswith(blueprint_relative.rstrip('/')+'/'):
            candidates.append(path[len(blueprint_relative.rstrip('/'))+1:])
        if not any(allowed(p,patterns) for p in candidates): bad.append(path)
    return sorted(set(bad))


def git(repo: Path, args: list[str]) -> bytes:
    result=subprocess.run(['git','-C',str(repo),*args],stdout=subprocess.PIPE,stderr=subprocess.PIPE,
                          timeout=30,check=False,shell=False)
    if result.returncode: raise ValueError('Git inspection failed: '+result.stderr.decode('utf-8','replace')[:2000])
    return result.stdout


def main() -> int:
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--task',required=True);p.add_argument('--repo-root',type=Path,required=True)
    p.add_argument('--base',required=True,help='Commit/revision from before this task; not an assumed HEAD.')
    args=p.parse_args();repo=args.repo_root.resolve()
    tasks=json.loads((ROOT/'docs/plan/TASKS.json').read_text(encoding='utf-8'))['tasks']
    task=next((t for t in tasks if t['id']==args.task),None)
    if task is None: p.error('unknown task ID')
    try:
        actual=Path(git(repo,['rev-parse','--show-toplevel']).decode().strip()).resolve()
        if actual!=repo: raise ValueError('--repo-root must be the repository top level')
        base=git(repo,['rev-parse','--verify','--end-of-options',args.base+'^{commit}']).decode().strip()
        changed=git(repo,['diff','--name-only','-z',base,'--'])+git(repo,['ls-files','--others','--exclude-standard','-z'])
        paths=sorted(set(p.decode('utf-8','surrogateescape') for p in changed.split(b'\0') if p))
        relative=ROOT.relative_to(repo).as_posix() if ROOT.is_relative_to(repo) and ROOT!=repo else None
        bad=check_paths(paths,task,relative)
        report={'status':'FAIL' if bad else 'PASS_PATH_SCOPE_ONLY','task':args.task,'resolvedBase':base,
                'changedPaths':paths,'outOfScope':bad,'semanticReviewStillRequired':True}
        print(json.dumps(report,indent=2,ensure_ascii=True));return 1 if bad else 0
    except (OSError,ValueError,subprocess.TimeoutExpired) as exc:
        print(json.dumps({'status':'BLOCKED','reason':str(exc)},indent=2));return 2

if __name__=='__main__': raise SystemExit(main())
