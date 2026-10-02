# P05-T03 review record

Reviewer: independent review subagent with security focus (owner-designated 2026-10-02).

## Round 1 — CHANGES_REQUIRED (no path for a corrupt/unverified snapshot to pass)
- M1 restore layer only proven on the happy path → thin/extra-ref/truncated/manifest
  shape tests with rewritten checksums (N8, N9).
- L-M2 fixed temp names → unique verify dir; single-writer create documented.
- M3 gate trusts ChangeKind → contract documented; P06 must derive kind from ancestry.
- L4 manifest kind/version/repo/namespace unchecked → checked.
- L5 staging left on rename failure → removed; restore partial dest documented.
Runner bundle additions: no weakening found.

## Round 2 — ACCEPTED, subject to OWNER acceptance of residual risks
1. Unauthenticated manifest/sha/bundle (local writer to backup root can forge).
2. Full restore per gate-time verify (disk/time); ENOSPC classification unit-tested only,
   real full disk not exercised.
3. create single-writer per repository by convention (fails closed if violated).
4. Crash can leave orphan `<id>.verify-*` / `<id>.staging` dirs (disk leak; no GC yet).
5. Bundle fetch uses file transport; swap race limited by exact ref match + fsck.
6. restore may leave a partial repo in the user's destination; check-then-use gap.
7. Gate trusts ChangeKind until P06 derives it from ancestry/observed state.
8. Snapshots cover source-mirror heads/tags only; no destination-only state, HEAD,
   other namespaces; LFS/submodules not_covered.

Owner decision: ACCEPTED residual risks 1–8 (owner answer in session, 2026-10-02:
"Chấp nhận, làm tiếp"). Follow-ups tracked in IMPLEMENTATION.json.
