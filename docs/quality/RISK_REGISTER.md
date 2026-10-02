# Risk register

| Risk | Required mitigation | Evidence |
|---|---|---|
| Source compromise/deletion propagates | exact plans, review, pre-change full snapshot, no auto cleanup | TEST-PLAN-LEASE, TEST-SNAPSHOT, TEST-BUNDLE-RESTORE |
| Mirror falsely called full backup | refs/LFS/submodule scope separated; restore check | TEST-REFS, TEST-LFS-RESTORE, TEST-SUBMODULE |
| Token appears in argv/log/UI | scoped broker, OS keyring, redaction, no renderer persistence | TEST-CREDENTIAL-BROKER, TEST-REDACTION |
| Malicious repo config/URLs | sanitized Git config, explicit host/protocol policy, no implicit hooks/recursion | TEST-GIT-INPUT |
| Crash leaves duplicate/partial work | durable jobs, per-repo lock, observation after uncertainty | TEST-CRASH, TEST-PROJECT-CREATE |
| Public backup of private source | private default and explicit reviewed visibility transition | TEST-PRIVACY |
| UI copied with bugs or invented controls | approved baseline + screen contracts + actual adapter gating | TEST-VISUAL, TEST-NO-FAKES |
| Figma quota blocks fidelity | record blocker; proceed only independent tasks | P02-T01 remains BLOCKED until actual evidence |
| Browser tests mask Linux failures | native Arch/Hyprland checks and capability fallbacks | TEST-NATIVE-UI, TEST-LIFECYCLE |
| AI changes tests to bless itself | separate review, artifact hashes, traceability | TEST-NO-FAKES, release review |

These mitigations reduce specific risks, not a guarantee against every data-loss or AI error.
The agent must report newly discovered outcome-relevant risks rather than silently expanding
scope or weakening the gate.
