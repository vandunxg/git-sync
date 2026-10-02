# Review — P04-T01 paginated repository discovery

Reviewer: independent review subagent (owner-designated 2026-10-02). Two rounds.
Round 1 CHANGES_REQUIRED: H1 Link header split on commas could truncate the next URL (echoed
`affiliation=a,b`) and end the walk as falsely complete; M2 known repos merely out of scope were flagged
source_unavailable; M3 per_page=100 vs 1 MiB body cap; L4 relation inference for outside collaborators on org
repos; L5 offset-pagination caveat; N6 login must come from the validated identity.
Fixes: RFC 8288 `<...>` scanning across all Link headers, fail-closed parsing, %2C affiliation; out_of_scope ID
bucket retained; PER_PAGE 50 / 200 pages; non-owned kept when either opt-in is on; documented caveats.
Round 2 ACCEPTED. Nits: a `, <` sequence inside a parameter value would misparse (fail-closed; not produced by
GitHub); multiple next links rejected (strict, safe).
Not in scope: catalog persistence (later task); live GitHub run (no network test against personal accounts).
