//! `RefWriter` (exact-ref pushes with leases) and `AncestryOracle` over the app's
//! bare mirror (P06-T03).
//!
//! `git push --porcelain [--atomic] --no-follow-tags --recurse-submodules=no
//! --no-verify --force-with-lease=<ref>:<oid|> ... -- <destination> <oid>:<ref> | :<ref>`
//! runs in the mirror (`GIT_DIR`), so the only write target is the destination
//! location given here; the source is never a push target. Results come from Git's
//! porcelain report (`<flag>\t<from>:<to>\t<summary>`, ending in `Done`), checked
//! against the refs that were sent. Anything incomplete or unparsable is `Unknown`.

use std::path::PathBuf;

use gbm_application::apply::{PushError, PushReport, RefPushStatus, RefWriter, RejectReason};
use gbm_domain::plan::{AncestryOracle, RefOperation};
use gbm_domain::refs::ObjectId;

use crate::git_auth::{AllowedHost, RemoteTarget};

use super::runner::{
    Arg, AuthArgs, CancelToken, GitInvocation, GitOutput, GitRunner, Location, PushSpec, RunError,
    RunLimits, Subcommand, Termination, WorkDir,
};

const ATOMIC_UNSUPPORTED: &str = "the receiving end does not support --atomic push";

pub struct GitPusher {
    runner: GitRunner,
    mirror: PathBuf,
    destination: Location,
    auth: Option<AuthArgs>,
    limits: RunLimits,
    cancel: CancelToken,
}

impl GitPusher {
    /// The destination must be a GitLab remote: the source host (GitHub) and local
    /// paths are refused, so a wiring mistake cannot turn a plan into a source write.
    pub fn new(
        runner: GitRunner,
        mirror: PathBuf,
        destination: RemoteTarget,
        auth: Option<AuthArgs>,
        limits: RunLimits,
        cancel: CancelToken,
    ) -> Result<Self, RunError> {
        if destination.host() != AllowedHost::GitLab {
            return Err(RunError::InvalidLocation);
        }
        Ok(Self {
            runner,
            mirror,
            destination: Location::Remote(destination),
            auth,
            limits,
            cancel,
        })
    }

    /// Test-only: a local bare repository as the destination.
    #[cfg(test)]
    pub(crate) fn to_local_for_tests(
        runner: GitRunner,
        mirror: PathBuf,
        destination: Location,
        limits: RunLimits,
        cancel: CancelToken,
    ) -> Self {
        Self {
            runner,
            mirror,
            destination,
            auth: None,
            limits,
            cancel,
        }
    }

    fn invocation(
        &self,
        operations: &[RefOperation],
        atomic: bool,
    ) -> Result<GitInvocation, RunError> {
        let mut args = vec![
            Arg::Flag("--porcelain"),
            Arg::Flag("--no-follow-tags"),
            Arg::Flag("--recurse-submodules=no"),
            Arg::Flag("--no-verify"),
        ];
        if atomic {
            args.push(Arg::Flag("--atomic"));
        }
        for op in operations {
            args.push(Arg::Lease(op.name.clone(), op.expected_destination.clone()));
        }
        args.push(Arg::EndOfOptions);
        args.push(Arg::Location(self.destination.clone()));
        for op in operations {
            args.push(Arg::PushSpec(match &op.source {
                Some(oid) => PushSpec::update(oid, &op.name),
                None => PushSpec::delete(&op.name),
            }));
        }
        let invocation = GitInvocation::new(Subcommand::Push, args)?;
        Ok(match &self.auth {
            Some(auth) => invocation.with_auth(auth.clone()),
            None => invocation,
        })
    }
}

/// Reason for a `!` line. Client-side `[rejected]` reasons are fixed strings; the text
/// after `[remote rejected]` comes from the server and is only a label.
/// `[remote failure]` (the server did not report this ref) means the ref may have been
/// applied: `None`, so the whole push is treated as unknown.
fn reject_reason(summary: &str) -> Option<RejectReason> {
    if let Some(reason) = summary.strip_prefix("[rejected] ") {
        Some(match reason {
            "(stale info)" => RejectReason::StaleLease,
            "(atomic push failed)" => RejectReason::AtomicAborted,
            "(non-fast-forward)" | "(fetch first)" => RejectReason::NonFastForward,
            _ => RejectReason::Other,
        })
    } else if let Some(reason) = summary.strip_prefix("[remote rejected]") {
        Some(if reason.trim() == "(atomic push failure)" {
            RejectReason::AtomicAborted
        } else {
            RejectReason::RemoteRejected
        })
    } else if summary.starts_with("[remote failure]") {
        None
    } else {
        Some(RejectReason::Other)
    }
}

/// Parses `push --porcelain` stdout into per-ref statuses in `operations` order.
/// `None` when the report is incomplete, ambiguous or mentions other refs.
pub(crate) fn parse_porcelain(text: &str, operations: &[RefOperation]) -> Option<PushReport> {
    let mut statuses: Vec<Option<RefPushStatus>> = vec![None; operations.len()];
    let mut done = false;
    for line in text.lines() {
        if line == "Done" {
            done = true;
            continue;
        }
        if line.starts_with("To ") {
            continue;
        }
        let mut fields = line.splitn(3, '\t');
        let (Some(flag), Some(refs), Some(summary)) = (fields.next(), fields.next(), fields.next())
        else {
            return None;
        };
        let to = refs.rsplit_once(':').map(|(_, to)| to)?;
        let index = operations.iter().position(|op| op.name.as_str() == to)?;
        let status = match flag {
            " " | "+" | "-" | "*" => RefPushStatus::Updated,
            "=" => RefPushStatus::UpToDate,
            "!" => RefPushStatus::Rejected(reject_reason(summary)?),
            _ => return None,
        };
        if statuses[index].replace(status).is_some() {
            return None;
        }
    }
    if !done {
        return None;
    }
    let refs = operations
        .iter()
        .zip(statuses)
        .map(|(op, status)| status.map(|s| (op.name.clone(), s)))
        .collect::<Option<Vec<_>>>()?;
    Some(PushReport { refs })
}

fn classify_output(
    out: &GitOutput,
    operations: &[RefOperation],
    atomic: bool,
) -> Result<PushReport, PushError> {
    if !out.reaped || !out.output_complete {
        return Err(PushError::Unknown);
    }
    match out.termination {
        Termination::Exited(128)
            if atomic
                && out
                    .stderr
                    .lines()
                    .any(|l| l.trim_end() == format!("fatal: {ATOMIC_UNSUPPORTED}")) =>
        {
            Err(PushError::AtomicUnsupported)
        }
        Termination::Exited(code @ (0 | 1)) => {
            let text = out.complete_stdout().map_err(|_| PushError::Unknown)?;
            let report = parse_porcelain(text, operations).ok_or(PushError::Unknown)?;
            let rejected = report
                .refs
                .iter()
                .any(|(_, s)| matches!(s, RefPushStatus::Rejected(_)));
            // Exit status and report must agree.
            if (code == 0) == rejected {
                return Err(PushError::Unknown);
            }
            Ok(report)
        }
        _ => Err(PushError::Unknown),
    }
}

impl RefWriter for GitPusher {
    fn push(&self, operations: &[RefOperation], atomic: bool) -> Result<PushReport, PushError> {
        if operations.is_empty() {
            return Ok(PushReport { refs: Vec::new() });
        }
        let invocation = self
            .invocation(operations, atomic)
            .map_err(|_| PushError::NotStarted)?;
        let out = match self.runner.run(
            &invocation,
            &WorkDir::GitDir(self.mirror.clone()),
            self.limits,
            &self.cancel,
            &[],
        ) {
            Ok(out) => out,
            // Rejected before spawning: nothing reached the destination.
            Err(
                RunError::Cancelled
                | RunError::InvalidArgument
                | RunError::InvalidWorkDir
                | RunError::Spawn,
            ) => {
                return Err(PushError::NotStarted);
            }
            Err(_) => return Err(PushError::Unknown),
        };
        classify_output(&out, operations, atomic)
    }
}

/// Ancestry from the mirror's object store via `merge-base --is-ancestor`.
/// Exit 0 = ancestor, 1 = not; anything else (missing objects, errors) = unknown.
pub struct MirrorAncestry {
    runner: GitRunner,
    mirror: PathBuf,
    limits: RunLimits,
    cancel: CancelToken,
}

impl MirrorAncestry {
    pub fn new(runner: GitRunner, mirror: PathBuf, limits: RunLimits, cancel: CancelToken) -> Self {
        Self {
            runner,
            mirror,
            limits,
            cancel,
        }
    }
}

impl AncestryOracle for MirrorAncestry {
    fn is_ancestor(&self, ancestor: &ObjectId, descendant: &ObjectId) -> Option<bool> {
        let invocation = GitInvocation::new(
            Subcommand::MergeBase,
            vec![
                Arg::Flag("--is-ancestor"),
                Arg::Oid(ancestor.clone()),
                Arg::Oid(descendant.clone()),
            ],
        )
        .ok()?;
        let out = self
            .runner
            .run(
                &invocation,
                &WorkDir::GitDir(self.mirror.clone()),
                self.limits,
                &self.cancel,
                &[],
            )
            .ok()?;
        match (out.termination, out.reaped, out.output_complete) {
            (Termination::Exited(0), true, true) => Some(true),
            (Termination::Exited(1), true, true) if out.stderr.is_empty() => Some(false),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests;
