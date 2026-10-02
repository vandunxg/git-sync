#!/usr/bin/env python3
"""Check documentation registries, local links, DTO fixtures and reference SQL."""
from __future__ import annotations
import argparse
import json
import re
import sqlite3
from pathlib import Path
from typing import Any
from urllib.parse import unquote
from contract_checks import schema_errors, validate

ROOT = Path(__file__).resolve().parents[1]


def acyclic(graph: dict[str, list[str]]) -> bool:
    active: set[str] = set(); done: set[str] = set()
    def visit(node: str) -> bool:
        if node in active: return False
        if node in done: return True
        active.add(node)
        for nxt in graph[node]:
            if nxt not in graph or not visit(nxt): return False
        active.remove(node); done.add(node); return True
    return all(visit(n) for n in graph)


def lint(root: Path = ROOT) -> dict[str, Any]:
    errors: list[str] = []; warnings: list[str] = []
    loaded: dict[str, Any] = {}
    for path in root.rglob('*.json'):
        if 'local' in path.relative_to(root).parts or '__pycache__' in path.parts: continue
        try: loaded[path.relative_to(root).as_posix()] = json.loads(path.read_text(encoding='utf-8'))
        except (ValueError, OSError) as exc: errors.append(f'{path.name}: invalid JSON: {exc}')
    required = ['docs/spec/REQUIREMENTS.json', 'docs/plan/PHASES.json', 'docs/plan/TASKS.json',
                'docs/quality/TESTS.json', 'docs/ui/SCREENS.json', 'contracts/ipc.v1.json',
                'contracts/dtos.schema.json', 'fixtures/ui-seed.json', 'docs/ui/BASELINES.json',
                'quality/GATES.json', 'docs/architecture/DECISIONS.json', 'project-manifest.json']
    for name in required:
        if name not in loaded: errors.append(f'missing required registry {name}')
    if any(name not in loaded for name in required): return {'status':'FAIL','errors':errors,'warnings':warnings}
    def indexed(path: str, key: str) -> dict[str, Any]:
        entries = loaded[path][key]; out = {v['id']:v for v in entries}
        if len(out) != len(entries): errors.append(f'{path}: duplicate IDs')
        return out
    reqs=indexed('docs/spec/REQUIREMENTS.json','requirements')
    phases=indexed('docs/plan/PHASES.json','phases')
    tasks=indexed('docs/plan/TASKS.json','tasks')
    tests=indexed('docs/quality/TESTS.json','tests')
    screens=indexed('docs/ui/SCREENS.json','screens')
    adrs=indexed('docs/architecture/DECISIONS.json','decisions')
    for label, registry in [('phase', phases),('task',tasks)]:
        graph={i:v.get('dependsOn',[]) for i,v in registry.items()}
        if not acyclic(graph): errors.append(f'{label}: unresolved dependency or cycle')
    for pid, phase in phases.items():
        if not (root/phase['doc']).is_file(): errors.append(f'{pid}: missing phase document')
        for tid in phase['tasks']:
            if tid not in tasks or tasks[tid]['phase'] != pid: errors.append(f'{pid}: task mismatch {tid}')
    for tid,t in tasks.items():
        if t['phase'] not in phases: errors.append(f'{tid}: invalid phase')
        if t.get('phaseDependencies') != phases.get(t['phase'],{}).get('dependsOn'): errors.append(f'{tid}: phase dependencies drift')
        if not t.get('allowedPaths'): errors.append(f'{tid}: missing scope')
        for rid in t['requirements']:
            if rid not in reqs or tid not in reqs[rid]['task_ids']: errors.append(f'{tid}: invalid requirement link {rid}')
        for test in t['tests']:
            if test not in tests: errors.append(f'{tid}: unknown test {test}')
    for rid,r in reqs.items():
        if not r['test_ids'] or not r['task_ids']: errors.append(f'{rid}: missing implementation/test coverage')
        for tid in r['task_ids']:
            if tid not in tasks or rid not in tasks[tid]['requirements']: errors.append(f'{rid}: task reverse link {tid}')
        for test in r['test_ids']:
            if test not in tests or rid not in tests[test]['requirements']: errors.append(f'{rid}: test reverse link {test}')
    for test,t in tests.items():
        for rid in t['requirements']:
            if rid not in reqs or test not in reqs[rid]['test_ids']: errors.append(f'{test}: invalid requirement {rid}')
    for aid,a in adrs.items():
        if not (root/a['doc']).is_file(): errors.append(f'{aid}: missing ADR document')
        for rid in a['requirements']:
            if rid not in reqs: errors.append(f'{aid}: invalid requirement {rid}')
    ipc=loaded['contracts/ipc.v1.json']; schema=loaded['contracts/dtos.schema.json']
    errors += schema_errors(schema); defs=schema['$defs']
    cmds={c['name']:c for c in ipc['commands']}
    if len(cmds)!=len(ipc['commands']): errors.append('duplicate IPC command')
    for name,c in cmds.items():
        for field in ['requestType','responseType']:
            if c[field] not in defs: errors.append(f'{name}: unknown {field}')
        if c['requirementId'] not in reqs: errors.append(f'{name}: unknown requirement')
    if ipc['events']['type'] not in defs: errors.append('unknown event DTO')
    for sid,s in screens.items():
        if not (root/s['spec']).is_file(): errors.append(f'{sid}: missing spec')
        if s['testId'] not in tests: errors.append(f'{sid}: missing test')
        for rid in s['requirements']:
            if rid not in reqs: errors.append(f'{sid}: unknown requirement {rid}')
        for command in s['commands']:
            if command not in cmds: errors.append(f'{sid}: unknown command {command}')
    fixture=loaded['fixtures/ui-seed.json']
    for sample in fixture['dtoFixtures']:
        if sample['type'] not in defs: errors.append('fixture unknown type'); continue
        errors += validate(sample['value'],defs[sample['type']],schema,sample['type'])
    by_type={x['type']:x['value'] for x in fixture['dtoFixtures']}
    if 'Dashboard' in by_type and 'RepoListResult' in by_type:
        dash=by_type['Dashboard']; repos=by_type['RepoListResult']['items']
        if dash['tracked'] != len(repos): errors.append('fixture dashboard tracked count mismatch')
        if dash['upToDate'] != sum(r['primaryStatus']=='synced' for r in repos): errors.append('fixture synced count mismatch')
        if dash['tracked'] != dash['upToDate']+dash['needsAttention']+dash['other']: errors.append('fixture dashboard sum mismatch')
    baselines=loaded['docs/ui/BASELINES.json']['screens']
    if {b['screenId'] for b in baselines}!=set(screens): errors.append('baseline screen registry mismatch')
    missing=sum(b.get('status')!='approved' for b in baselines)
    if missing: warnings.append(f'{missing} visual baselines are not approved; visual acceptance remains BLOCKED')
    try:
        with sqlite3.connect(':memory:') as connection:
            connection.executescript((root/'contracts/db.reference.sql').read_text(encoding='utf-8'))
            if connection.execute('PRAGMA foreign_key_check').fetchall(): errors.append('reference DDL foreign key errors')
    except (sqlite3.Error,OSError) as exc: errors.append(f'reference DDL failed: {exc}')
    # Only local Markdown links; omit external URLs and fragment-only anchors.
    link_pattern=re.compile(r'!?\[[^\]\n]*\]\(([^\s)]+)(?:\s+"[^"]*")?\)')
    for path in root.rglob('*.md'):
        if 'local' in path.relative_to(root).parts: continue
        content=path.read_text(encoding='utf-8')
        for target in link_pattern.findall(content):
            target=target.strip('<>')
            if re.match(r'^[a-zA-Z][a-zA-Z0-9+.-]*:',target) or target.startswith('#'): continue
            destination=unquote(target.split('#',1)[0])
            full=(path.parent/destination).resolve()
            if not full.is_relative_to(root.resolve()) or not full.exists(): errors.append(f'{path.relative_to(root)}: broken local link {target}')
    counts={'requirements':len(reqs),'phases':len(phases),'tasks':len(tasks),'plannedProductScenarios':len(tests),
            'screens':len(screens),'commands':len(cmds),'ADRs':len(adrs),'jsonFiles':len(loaded)}
    return {'status':'FAIL' if errors else 'PASS_BLUEPRINT_STRUCTURE_ONLY','counts':counts,'errors':errors,'warnings':warnings,
            'appImplemented':False,'productTestsRun':False}


def main() -> int:
    parser=argparse.ArgumentParser(description=__doc__); parser.add_argument('--root',type=Path,default=ROOT)
    args=parser.parse_args()
    try: result=lint(args.root.resolve())
    except (KeyError,TypeError,ValueError,OSError) as exc: result={'status':'FAIL','errors':[f'malformed registry: {exc}']}
    print(json.dumps(result,indent=2))
    return 1 if result['status']=='FAIL' else 0

if __name__=='__main__': raise SystemExit(main())
