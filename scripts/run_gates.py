#!/usr/bin/env python3
"""Run explicit quality profiles without a shell; no missing prerequisites pass."""
from __future__ import annotations
import argparse
import codecs
import json
import os
import re
import shutil
import signal
import subprocess
import sys
import threading
import time
import uuid
from datetime import datetime, timezone
from pathlib import Path
from typing import Any
from check_visual_baselines import audit as baseline_audit, contained_file

ROOT = Path(__file__).resolve().parents[1]


def redact(text: str) -> str:
    text = re.sub(r'(?i)(authorization\s*[:=]\s*)([^\r\n]+)', r'\1[REDACTED]', text)
    text = re.sub(r'(?i)(bearer\s+)[A-Za-z0-9._~+/=-]+', r'\1[REDACTED]', text)
    text = re.sub(r'(?i)\b(?:gh[pousr]_[A-Za-z0-9_]+|github_pat_[A-Za-z0-9_]+|glpat-[A-Za-z0-9_-]+)', '[REDACTED]', text)
    text = re.sub(r'(https?://)[^\s/@:]+:[^\s/@]+@', r'\1[REDACTED]@', text)
    text = re.sub(r'(?i)((?:access_token|refresh_token|password|secret)\s*[=:]\s*)[^\s,;]+', r'\1[REDACTED]', text)
    return text


def stop_process(process: subprocess.Popen[bytes]) -> None:
    """Kill the process group on POSIX; parent process on other platforms."""
    if os.name == 'posix':
        try: os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError: pass
    elif process.poll() is None:
        process.kill()
    try: process.wait(timeout=5)
    except subprocess.TimeoutExpired: pass


def run_command(argv: list[str], cwd: Path, log_path: Path, timeout: float = 1800, max_log_bytes: int = 1048576) -> dict[str, Any]:
    started = time.monotonic()
    public_argv = [redact(a) for a in argv]
    result: dict[str, Any] = {'argv': public_argv, 'cwd': str(cwd), 'log': str(log_path),
                             'startedAt': datetime.now(timezone.utc).isoformat()}
    log_path.parent.mkdir(parents=True, exist_ok=True)
    try:
        process = subprocess.Popen(argv, cwd=cwd, stdout=subprocess.PIPE,
                                   stderr=subprocess.STDOUT, stdin=subprocess.DEVNULL,
                                   shell=False, start_new_session=(os.name == 'posix'))
    except OSError as exc:
        result.update(status='BLOCKED', exitCode=None, error=redact(str(exc)), elapsedSeconds=0)
        log_path.write_text(redact(str(exc))+'\n', encoding='utf-8'); return result
    written = 0; dropped = False; reader_error: list[str] = []
    def drain() -> None:
        nonlocal written, dropped
        decoder=codecs.getincrementaldecoder('utf-8')('replace'); pending=''; discard_line=False
        def emit(fh: Any, line: str) -> None:
            nonlocal written, dropped
            data=redact(line).encode('utf-8')
            room=max(0,max_log_bytes-written)
            if len(data)>room: dropped=True
            if room: fh.write(data[:room]); written+=min(room,len(data))
        try:
            with log_path.open('wb') as fh:
                assert process.stdout is not None
                while True:
                    raw=process.stdout.read(4096)
                    if not raw: break
                    chunk=decoder.decode(raw)
                    pending+=chunk
                    while '\n' in pending:
                        line,pending=pending.split('\n',1)
                        if discard_line: discard_line=False
                        elif len(line)>16384: emit(fh,'[oversized output line omitted]\n')
                        else: emit(fh,line+'\n')
                    if len(pending)>16384:
                        if not discard_line: emit(fh,'[oversized output line omitted]\n')
                        pending='';discard_line=True
                pending+=decoder.decode(b'',final=True)
                if pending and not discard_line: emit(fh,pending+'\n')
                if dropped: fh.write(b'\n[log capped; remaining process output was drained]\n')
        except (OSError,ValueError) as exc: reader_error.append(redact(str(exc)))
    reader=threading.Thread(target=drain,daemon=True);reader.start()
    timed_out=False
    try: process.wait(timeout=timeout)
    except subprocess.TimeoutExpired:
        timed_out=True;stop_process(process)
    except BaseException:
        stop_process(process);reader.join(timeout=5);raise
    reader.join(timeout=5)
    stream_incomplete=reader.is_alive()
    if stream_incomplete:
        # Descendants must not keep pipes open after the owning command exits.
        stop_process(process);reader.join(timeout=5)
    if process.stdout is not None and not reader.is_alive(): process.stdout.close()
    failed=timed_out or stream_incomplete or bool(reader_error) or process.returncode!=0
    result.update(status='FAIL' if failed else 'PASS',exitCode=process.returncode,
                  timedOut=timed_out,streamIncomplete=stream_incomplete,logCapped=dropped,
                  readerErrors=reader_error,elapsedSeconds=round(time.monotonic()-started,3))
    return result


def preflight(profile: dict[str, Any], repo: Path, blueprint: Path = ROOT,
              allow_native: bool = False, sandbox_config: Path | None = None) -> list[str]:
    reasons=[]
    for name in profile.get('requireFiles',[]):
        if not (repo/name).is_file(): reasons.append(f'missing application prerequisite: {name}')
    needed=profile.get('packageScripts',[])
    if needed:
        try: package=json.loads((repo/'apps/desktop/package.json').read_text(encoding='utf-8'))
        except (OSError,ValueError): package={}
        for script in needed:
            if not isinstance(package.get('scripts',{}).get(script),str) or not package['scripts'][script].strip():
                reasons.append(f'missing application package script: {script}')
    for command in profile.get('commands',[]):
        executable=command['argv'][0]
        if executable!='{python}' and not shutil.which(executable): reasons.append(f'missing executable: {executable}')
    if profile.get('requiresBaselines'):
        result=baseline_audit(blueprint,require_goldens=True)
        if result['status']=='BLOCKED': reasons += result['reasons']
    if profile.get('requiresNativeAuthorization'):
        if not allow_native: reasons.append('native profile requires --allow-native')
        try:
            if sandbox_config is None: raise ValueError('missing --sandbox-config')
            config=json.loads(sandbox_config.read_text(encoding='utf-8'))
            if config.get('confirmedByOwner') is not True or config.get('targets')!='disposable_only':
                raise ValueError('sandbox must be owner-confirmed and disposable_only')
            contained_file(repo,config.get('authorizationEvidence'))
        except (OSError,ValueError,TypeError) as exc: reasons.append(f'native authorization: {exc}')
    return sorted(set(reasons))


def main() -> int:
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--profile',required=True)
    parser.add_argument('--repo-root',type=Path,default=Path.cwd())
    parser.add_argument('--evidence-dir',type=Path,default=ROOT/'state/evidence/local')
    parser.add_argument('--timeout',type=float,default=1800)
    parser.add_argument('--allow-native',action='store_true')
    parser.add_argument('--sandbox-config',type=Path)
    args=parser.parse_args()
    if args.timeout<=0: parser.error('timeout must be positive')
    profiles=json.loads((ROOT/'quality/GATES.json').read_text(encoding='utf-8'))['profiles']
    if args.profile not in profiles: parser.error('unknown profile; choose '+', '.join(profiles))
    profile=profiles[args.profile]; repo=args.repo_root.resolve()
    run_id=datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%SZ')+'-'+uuid.uuid4().hex[:8]
    evidence=args.evidence_dir.resolve()/run_id; evidence.mkdir(parents=True)
    reasons=preflight(profile,repo,ROOT,args.allow_native,args.sandbox_config)
    results=[]
    if not reasons:
        for index,command in enumerate(profile['commands']):
            argv=[sys.executable if a=='{python}' else a for a in command['argv']]
            cwd=ROOT if command.get('cwd')=='blueprint' else repo
            result=run_command(argv,cwd,evidence/f'{index:02d}-{command["name"]}.log',args.timeout)
            result['name']=command['name']; results.append(result)
    status='BLOCKED' if reasons or any(r['status']=='BLOCKED' for r in results) else ('FAIL' if any(r['status']=='FAIL' for r in results) else 'PASS')
    if not results and not reasons: status='BLOCKED';reasons=['profile has no executable checks']
    report={'profile':args.profile,'status':status,'reasons':reasons,'results':results,
            'generatedAt':datetime.now(timezone.utc).isoformat(),'repoRoot':str(repo),
            'notice':'A profile pass is not full product, visual, security or owner acceptance. Review task-specific scenarios and native evidence.'}
    (evidence/'report.json').write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8')
    print(json.dumps(report,indent=2)); print('Evidence: '+str(evidence/'report.json'))
    return {'PASS':0,'FAIL':1,'BLOCKED':2}[status]

if __name__=='__main__': raise SystemExit(main())
