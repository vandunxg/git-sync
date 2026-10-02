use super::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn map(entries: &[(&str, char)]) -> Result<RefMap, Box<dyn std::error::Error>> {
    let mut out = Vec::new();
    for (name, fill) in entries {
        out.push((
            RefName::parse(name).ok_or("name")?,
            ObjectId::parse(&fill.to_string().repeat(40)).ok_or("oid")?,
        ));
    }
    Ok(RefMap::from_entries(out)?)
}

#[test]
fn verification_is_by_observed_refs_not_exit_codes() -> TestResult {
    // TEST-VERIFY.
    let snapshot = map(&[("refs/heads/main", 'a'), ("refs/tags/v1", 'b')])?;
    assert_eq!(
        verify_destination(&snapshot, Some(&snapshot.clone())),
        Verification::Verified
    );
    assert_eq!(
        verify_destination(&snapshot, None),
        Verification::Unverified
    );
    let differs = map(&[("refs/heads/main", 'c'), ("refs/heads/extra", 'd')])?;
    let Verification::Mismatch(list) = verify_destination(&snapshot, Some(&differs)) else {
        return Err("expected mismatch".into());
    };
    let names: Vec<&str> = list.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["refs/heads/extra", "refs/heads/main", "refs/tags/v1"]
    );

    // Git said "applied" but a ref differs afterwards: failed, never success.
    let after = sync_result(
        PushResult::Applied,
        &Verification::Mismatch(list.clone()),
        false,
    );
    assert_eq!(after.result, JobResult::Failed);
    assert_eq!(after.error, Some(ErrorCode::VerifyFailed));
    assert!(!after.destination_verified);
    let partial = sync_result(PushResult::Partial, &Verification::Mismatch(list), false);
    assert_eq!(
        (partial.result, partial.error),
        (JobResult::Partial, Some(ErrorCode::PushPartial))
    );
    let unknown = sync_result(PushResult::Unknown, &Verification::Unverified, false);
    assert_eq!(unknown.result, JobResult::Failed);
    Ok(())
}

#[test]
fn a_newer_source_is_reported_without_undoing_a_verified_backup() {
    let result = sync_result(PushResult::Applied, &Verification::Verified, true);
    assert_eq!(result.result, JobResult::Succeeded);
    assert!(result.destination_verified && result.newer_source);
    // An uncertain push whose destination now matches is a verified backup.
    let reconciled = sync_result(PushResult::Unknown, &Verification::Verified, false);
    assert_eq!(reconciled.result, JobResult::Succeeded);
}

#[test]
fn rejections_map_to_actionable_codes() {
    let mismatch = Verification::Mismatch(Vec::new());
    for (push, code) in [
        (
            PushResult::Rejected {
                stale_lease: true,
                remote_rejected: false,
            },
            ErrorCode::PlanStale,
        ),
        (
            PushResult::Rejected {
                stale_lease: false,
                remote_rejected: true,
            },
            ErrorCode::ProtectedRef,
        ),
        (PushResult::Blocked, ErrorCode::Unsupported),
    ] {
        let result = sync_result(push, &mismatch, false);
        assert_eq!(
            (result.result, result.error),
            (JobResult::Failed, Some(code))
        );
    }
}

#[test]
fn destination_dimension() {
    let mismatch = Verification::Mismatch(Vec::new());
    assert_eq!(
        destination_state(&Verification::Verified, Some(true), false),
        DestinationState::Current
    );
    assert_eq!(
        destination_state(&mismatch, Some(true), false),
        DestinationState::Behind
    );
    assert_eq!(
        destination_state(&mismatch, Some(true), true),
        DestinationState::Diverged
    );
    assert_eq!(
        destination_state(&Verification::Verified, Some(false), false),
        DestinationState::Missing
    );
    assert_eq!(
        destination_state(&Verification::Unverified, Some(true), false),
        DestinationState::Unknown
    );
    assert_eq!(
        destination_state(&Verification::Verified, None, false),
        DestinationState::Unknown
    );
}

const SOURCES: [SourceState; 5] = [
    SourceState::Unknown,
    SourceState::Current,
    SourceState::Changed,
    SourceState::Unavailable,
    SourceState::Stale,
];
const MIRRORS: [MirrorState; 5] = [
    MirrorState::Missing,
    MirrorState::Staging,
    MirrorState::Current,
    MirrorState::Invalid,
    MirrorState::Unknown,
];
const DESTINATIONS: [DestinationState; 6] = [
    DestinationState::Missing,
    DestinationState::Current,
    DestinationState::Behind,
    DestinationState::Diverged,
    DestinationState::Inaccessible,
    DestinationState::Unknown,
];
const GIT_REFS: [CoverageGitRefs; 3] = [
    CoverageGitRefs::NotChecked,
    CoverageGitRefs::Verified,
    CoverageGitRefs::Failed,
];
const LOCAL: [CoverageLocalObjects; 3] = [
    CoverageLocalObjects::NotChecked,
    CoverageLocalObjects::Verified,
    CoverageLocalObjects::Failed,
];
const LFS: [CoverageLfs; 6] = [
    CoverageLfs::NotApplicable,
    CoverageLfs::NotChecked,
    CoverageLfs::LocalVerified,
    CoverageLfs::Uploaded,
    CoverageLfs::RestoreVerified,
    CoverageLfs::Incomplete,
];
const SUBMODULES: [CoverageSubmodules; 4] = [
    CoverageSubmodules::None,
    CoverageSubmodules::PointersOnly,
    CoverageSubmodules::SeparatelyCovered,
    CoverageSubmodules::Incomplete,
];

/// Every combination of every dimension (TEST-STATUS-AGGREGATION).
fn all_inputs() -> Vec<StatusInputs> {
    let mut out = Vec::new();
    for source in SOURCES {
        for mirror in MIRRORS {
            for destination in DESTINATIONS {
                for git_refs in GIT_REFS {
                    for local_objects in LOCAL {
                        for lfs in LFS {
                            for submodules in SUBMODULES {
                                for flags in 0..8u8 {
                                    for blocker in [None, Some(ErrorCode::AuthRequired)] {
                                        out.push(StatusInputs {
                                            source,
                                            mirror,
                                            destination,
                                            workspace_dirty_or_skipped: flags & 1 != 0,
                                            coverage: Coverage {
                                                git_refs,
                                                local_objects,
                                                lfs,
                                                submodules,
                                            },
                                            job_active: flags & 2 != 0,
                                            last_attempt_failed_unverified: flags & 4 != 0,
                                            blocker,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    out
}

#[test]
fn aggregation_is_deterministic_and_respects_precedence() {
    let inputs = all_inputs();
    assert!(inputs.len() > 100_000);
    for input in &inputs {
        let status = primary_status(input);
        assert_eq!(status, primary_status(input), "deterministic");
        // The workspace never changes the primary badge.
        let mut flipped = input.clone();
        flipped.workspace_dirty_or_skipped = !flipped.workspace_dirty_or_skipped;
        assert_eq!(primary_status(&flipped), status, "{input:?}");
        if input.job_active {
            assert_eq!(status, PrimaryStatus::Running);
            continue;
        }
        // A blocker is never hidden by a good old snapshot.
        if input.blocker.is_some() {
            assert_eq!(status, PrimaryStatus::Blocked, "{input:?}");
        }
        if status == PrimaryStatus::Synced {
            assert_eq!(input.source, SourceState::Current);
            assert_eq!(input.mirror, MirrorState::Current);
            assert_eq!(input.destination, DestinationState::Current);
            assert_eq!(input.coverage.git_refs, CoverageGitRefs::Verified);
            assert_eq!(input.coverage.local_objects, CoverageLocalObjects::Verified);
            assert!(matches!(
                input.coverage.lfs,
                CoverageLfs::NotApplicable | CoverageLfs::Uploaded | CoverageLfs::RestoreVerified
            ));
            assert_ne!(input.coverage.submodules, CoverageSubmodules::Incomplete);
            assert!(!input.last_attempt_failed_unverified && input.blocker.is_none());
        }
        // Pointer-only LFS is never a complete backup.
        if input.coverage.lfs == CoverageLfs::Incomplete {
            assert_eq!(status, PrimaryStatus::Blocked);
        }
        if input.last_attempt_failed_unverified {
            assert!(matches!(
                status,
                PrimaryStatus::Blocked | PrimaryStatus::Failed
            ));
        }
    }
}

#[test]
fn precedence_order_examples() {
    let healthy = StatusInputs {
        source: SourceState::Current,
        mirror: MirrorState::Current,
        destination: DestinationState::Current,
        workspace_dirty_or_skipped: true,
        coverage: Coverage {
            git_refs: CoverageGitRefs::Verified,
            local_objects: CoverageLocalObjects::Verified,
            lfs: CoverageLfs::NotApplicable,
            submodules: CoverageSubmodules::None,
        },
        job_active: false,
        last_attempt_failed_unverified: false,
        blocker: None,
    };
    assert_eq!(primary_status(&healthy), PrimaryStatus::Synced);
    let changed = StatusInputs {
        source: SourceState::Changed,
        ..healthy.clone()
    };
    assert_eq!(primary_status(&changed), PrimaryStatus::Update);
    let missing = StatusInputs {
        destination: DestinationState::Missing,
        ..changed.clone()
    };
    assert_eq!(primary_status(&missing), PrimaryStatus::Missing);
    let failed = StatusInputs {
        last_attempt_failed_unverified: true,
        ..missing.clone()
    };
    assert_eq!(primary_status(&failed), PrimaryStatus::Failed);
    let blocked = StatusInputs {
        blocker: Some(ErrorCode::AccessDenied),
        ..failed.clone()
    };
    assert_eq!(primary_status(&blocked), PrimaryStatus::Blocked);
    let running = StatusInputs {
        job_active: true,
        ..blocked
    };
    assert_eq!(primary_status(&running), PrimaryStatus::Running);
    let stale = StatusInputs {
        source: SourceState::Stale,
        ..healthy.clone()
    };
    assert_eq!(primary_status(&stale), PrimaryStatus::Stale);
    let unchecked = StatusInputs {
        coverage: Coverage {
            git_refs: CoverageGitRefs::NotChecked,
            ..healthy.coverage.clone()
        },
        ..healthy
    };
    assert_eq!(primary_status(&unchecked), PrimaryStatus::Unknown);
}
