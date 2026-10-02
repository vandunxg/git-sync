# Quality-gate utilities included in the archive

Requires Python 3.11+. No third-party Python package is needed.

```sh
python3 scripts/lint_pack.py
python3 -m unittest discover -s scripts/tests -v
python3 scripts/check_visual_baselines.py
python3 scripts/run_gates.py --profile docs --repo-root .
```

The first two commands check this documentation system and its helper code. They do not
execute the 54 planned product acceptance scenarios. The baseline checker initially returns
exit 2 (BLOCKED), because no owner-approved reference exists. App profiles initially return
BLOCKED because this archive has no Cargo/React application to test.

Once the actual app is scaffolded, run profiles against that repository:

```sh
python3 scripts/run_gates.py --profile core --repo-root /absolute/path/to/app
python3 scripts/run_gates.py --profile frontend --repo-root /absolute/path/to/app
python3 scripts/run_gates.py --profile visual --repo-root /absolute/path/to/app
```

Profiles are explicit in quality/GATES.json. Product scripts there are required interfaces to
implement in P01/P10, not a claim that the tools already provide such scripts. Inspect/version
pin dependencies and tailor only through a reviewed change. A missing tool, script, baseline
or manifest blocks the entire profile. An executed failure fails it. Native/provider tests
need explicit authorized sandbox configuration and the --allow-native flag where applicable.

The runner uses argument arrays, not a shell. It writes per-run reports and bounded redacted
output under state/evidence/local/ unless a different evidence root is chosen. Synthetic
examples aside, do not put real secrets in test output: generic redaction cannot recognize
every secret. The orchestrator itself performs no GitHub/GitLab mutations, but configured
application test scripts can; therefore only run reviewed scripts against authorized targets.

Contract checking is a deliberately bounded schema subset covering the constructs used in
contracts/dtos.schema.json. It is not a general JSON Schema compliance engine. P01 must use
the chosen actual Rust/TypeScript validation/type pipeline and parity tests. The package lint
also executes db.reference.sql against an in-memory SQLite connection as a reference-DDL check;
that is not a production migration/recovery test.

Visual baseline file integrity is not pixel comparison, UX review or proof of human consent.
The visual profile also invokes the actual application's visual suite; owner review and
native evidence remain required. No software checklist can guarantee that every agent always
obeys instructions; keep review and external/destructive permissions outside agent discretion.

## Task path-scope guard

Record the task's starting commit, then run:

```sh
python3 scripts/check_task_scope.py --task P02-T03 --repo-root /absolute/path/to/app --base TASK_START_COMMIT
```

The guard includes tracked changes since that base and untracked, non-ignored files. It checks exact files/directory prefixes from TASKS.json plus common state/evidence paths, including a nested blueprint folder. It does not prove correct semantics or authorize out-of-scope work. Ignored build artifacts are not inspected. The required base must identify the tree before this task, not a convenient later commit that hides changes. Review any pre-existing owner changes rather than resetting them.
