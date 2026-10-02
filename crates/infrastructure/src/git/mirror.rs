//! Git adapters for snapshot capture (P05-T02): `GitMirror` implements the application
//! `MirrorGit` port on one app-owned bare mirror, and `RunnerRefReader` implements the
//! read-only `SourceRefReader` with `ls-remote`. Every call goes through `GitRunner`
//! (typed arguments, hardening, bounded output, cancellation). The mirror directory
//! must come from `OwnedRoot::create_dir`/`open_dir`; nothing here picks paths.

use std::path::PathBuf;

use gbm_application::observation::{ReadError, SourceRefReader};
use gbm_application::snapshot::{MirrorError, MirrorGit};
use gbm_domain::refs::{ObjectFormat, ObjectId, RefName};

use super::runner::{
    Arg, AuthArgs, CancelToken, FullRef, GitInvocation, GitOutput, GitRunner, Location, RefPrefix,
    RefSpec, RunError, RunLimits, Subcommand, Termination, WorkDir,
};

/// Refspecs per `git fetch` (argument-count bound of the runner).
const REFSPECS_PER_FETCH: usize = 200;
const FETCH_FLAGS: [&str; 6] = [
    "--no-tags",
    "--no-write-fetch-head",
    "--no-recurse-submodules",
    "--no-auto-gc",
    "--no-auto-maintenance",
    "--quiet",
];
const REF_FORMAT: &str = "--format=%(objectname)%09%(objecttype)%09%(refname)";

fn format_word(format: ObjectFormat) -> &'static str {
    match format {
        ObjectFormat::Sha1 => "sha1",
        ObjectFormat::Sha256 => "sha256",
    }
}

/// One job's view of a bare mirror and its source.
pub struct GitMirror {
    runner: GitRunner,
    dir: PathBuf,
    source: Location,
    auth: Option<AuthArgs>,
    limits: RunLimits,
    cancel: CancelToken,
}

impl GitMirror {
    pub fn new(
        runner: GitRunner,
        dir: PathBuf,
        source: Location,
        auth: Option<AuthArgs>,
        limits: RunLimits,
        cancel: CancelToken,
    ) -> Self {
        Self {
            runner,
            dir,
            source,
            auth,
            limits,
            cancel,
        }
    }

    fn run_with(
        &self,
        invocation: GitInvocation,
        cancel: &CancelToken,
    ) -> Result<GitOutput, MirrorError> {
        let out = self
            .runner
            .run(
                &invocation,
                &WorkDir::GitDir(self.dir.clone()),
                self.limits,
                cancel,
                &[],
            )
            .map_err(|error| match error {
                RunError::Cancelled => MirrorError::Cancelled,
                _ => MirrorError::Failed,
            })?;
        if matches!(out.termination, Termination::Cancelled) {
            return Err(MirrorError::Cancelled);
        }
        Ok(out)
    }

    fn stdout(&self, invocation: GitInvocation) -> Result<String, MirrorError> {
        self.stdout_with(invocation, &self.cancel)
    }

    fn stdout_with(
        &self,
        invocation: GitInvocation,
        cancel: &CancelToken,
    ) -> Result<String, MirrorError> {
        let out = self.run_with(invocation, cancel)?;
        out.success_stdout()
            .map(str::to_owned)
            .map_err(|_| MirrorError::Failed)
    }

    fn invocation(subcommand: Subcommand, args: Vec<Arg>) -> Result<GitInvocation, MirrorError> {
        GitInvocation::new(subcommand, args).map_err(|_| MirrorError::Failed)
    }

    fn fetch(
        &self,
        from: &Location,
        pairs: &[(String, String)],
        auth: bool,
        atomic: bool,
    ) -> Result<(), MirrorError> {
        for chunk in pairs.chunks(REFSPECS_PER_FETCH) {
            let mut args: Vec<Arg> = FETCH_FLAGS.iter().map(|flag| Arg::Flag(flag)).collect();
            if atomic {
                args.push(Arg::Flag("--atomic"));
            }
            args.push(Arg::EndOfOptions);
            args.push(Arg::Location(from.clone()));
            for (source, destination) in chunk {
                let spec =
                    RefSpec::new(source, destination, true).map_err(|_| MirrorError::Failed)?;
                args.push(Arg::RefSpec(spec));
            }
            let mut invocation = Self::invocation(Subcommand::Fetch, args)?;
            if auth && let Some(words) = &self.auth {
                invocation = invocation.with_auth(words.clone());
            }
            self.stdout(invocation)?;
        }
        Ok(())
    }
}

impl MirrorGit for GitMirror {
    fn prepare(&self, format: ObjectFormat) -> Result<(), MirrorError> {
        if self.dir.join("HEAD").exists() {
            let existing = self.stdout(Self::invocation(
                Subcommand::RevParse,
                vec![Arg::Flag("--show-object-format")],
            )?)?;
            return if existing.trim_end() == format_word(format) {
                Ok(())
            } else {
                Err(MirrorError::ObjectFormatMismatch)
            };
        }
        let flag = match format {
            ObjectFormat::Sha1 => "--object-format=sha1",
            ObjectFormat::Sha256 => "--object-format=sha256",
        };
        self.stdout(Self::invocation(
            Subcommand::Init,
            vec![Arg::Flag("--bare"), Arg::Flag("--quiet"), Arg::Flag(flag)],
        )?)
        .map(|_| ())
    }

    fn fetch_from_source(&self, pairs: &[(RefName, String)]) -> Result<(), MirrorError> {
        let pairs: Vec<(String, String)> = pairs
            .iter()
            .map(|(name, local)| (name.as_str().to_owned(), local.clone()))
            .collect();
        self.fetch(&self.source, &pairs, true, false)
    }

    fn copy_refs(&self, pairs: &[(String, String)]) -> Result<(), MirrorError> {
        let own = Location::local(&self.dir).map_err(|_| MirrorError::Failed)?;
        // Atomic per batch; the caller verifies the whole namespace and removes it on
        // any failure, so a multi-batch partial copy is never kept.
        self.fetch(&own, pairs, false, true)
    }

    fn read_refs(&self, prefix: &str) -> Result<Vec<(String, ObjectId)>, MirrorError> {
        self.read_with(prefix, &self.cancel)
    }

    fn check_integrity(&self) -> Result<(), MirrorError> {
        self.stdout(Self::invocation(
            Subcommand::Fsck,
            vec![Arg::Flag("--no-dangling"), Arg::Flag("--no-progress")],
        )?)
        .map(|_| ())
    }

    fn delete_refs(&self, refs: &[(String, ObjectId)]) -> Result<(), MirrorError> {
        self.delete_with(refs, &self.cancel)
    }

    fn remove_namespace(&self, prefix: &str) -> Result<(), MirrorError> {
        // Cleanup must not be skipped because the job token was cancelled.
        let cleanup = CancelToken::default();
        if !is_app_namespace(prefix) {
            return Err(MirrorError::Failed);
        }
        if !self.dir.join("HEAD").exists() {
            return Ok(()); // never prepared: nothing to remove
        }
        let refs = self.read_with(prefix, &cleanup)?;
        self.delete_with(&refs, &cleanup)
    }
}

/// Only refs inside one id-qualified app namespace (`refs/gbm/staging/<id>/` or
/// `refs/gbm/snapshots/<id>/`) may be read for cleanup or deleted; the bare
/// `refs/gbm/snapshots/` (all snapshots) is refused.
fn is_app_namespace(name: &str) -> bool {
    ["refs/gbm/staging/", "refs/gbm/snapshots/"]
        .iter()
        .filter_map(|base| name.strip_prefix(base))
        .any(|rest| rest.split_once('/').is_some_and(|(id, _)| !id.is_empty()))
}

impl GitMirror {
    fn read_with(
        &self,
        prefix: &str,
        cancel: &CancelToken,
    ) -> Result<Vec<(String, ObjectId)>, MirrorError> {
        let pattern = RefPrefix::new(prefix).map_err(|_| MirrorError::Failed)?;
        let text = self.stdout_with(
            Self::invocation(
                Subcommand::ForEachRef,
                vec![Arg::Flag(REF_FORMAT), Arg::RefPrefix(pattern)],
            )?,
            cancel,
        )?;
        parse_ref_lines(&text, prefix)
    }

    fn delete_with(
        &self,
        refs: &[(String, ObjectId)],
        cancel: &CancelToken,
    ) -> Result<(), MirrorError> {
        for (name, expected) in refs {
            if !is_app_namespace(name) {
                return Err(MirrorError::Failed);
            }
            let name = FullRef::new(name).map_err(|_| MirrorError::Failed)?;
            self.stdout_with(
                Self::invocation(
                    Subcommand::UpdateRef,
                    vec![
                        Arg::Flag("--no-deref"),
                        Arg::Flag("-d"),
                        Arg::Ref(name),
                        Arg::Oid(expected.clone()),
                    ],
                )?,
                cancel,
            )?;
        }
        Ok(())
    }
}

/// Parses `for-each-ref` lines `<oid>\t<type>\t<name>`; every name must be under
/// `prefix`. Anything else fails closed.
fn parse_ref_lines(text: &str, prefix: &str) -> Result<Vec<(String, ObjectId)>, MirrorError> {
    let mut refs = Vec::new();
    for line in text.lines() {
        let mut fields = line.split('\t');
        let (Some(oid), Some(_kind), Some(name), None) =
            (fields.next(), fields.next(), fields.next(), fields.next())
        else {
            return Err(MirrorError::Failed);
        };
        let oid = ObjectId::parse(oid).ok_or(MirrorError::Failed)?;
        if !name.starts_with(prefix) {
            return Err(MirrorError::Failed);
        }
        refs.push((name.to_owned(), oid));
    }
    Ok(refs)
}

/// `SourceRefReader` over `git ls-remote --symref` through the runner.
pub struct RunnerRefReader {
    runner: GitRunner,
    location: Location,
    auth: Option<AuthArgs>,
    limits: RunLimits,
    cancel: CancelToken,
}

impl RunnerRefReader {
    pub fn new(
        runner: GitRunner,
        location: Location,
        auth: Option<AuthArgs>,
        limits: RunLimits,
        cancel: CancelToken,
    ) -> Self {
        Self {
            runner,
            location,
            auth,
            limits,
            cancel,
        }
    }
}

/// Maps Git's (redacted) stderr of a failed listing to a sanitized class, using Git's
/// own message phrases (not bare substrings such as "not found" or "403").
fn classify(stderr: &str) -> ReadError {
    let text = stderr.to_ascii_lowercase();
    let not_found = [
        "fatal: repository not found",
        "does not appear to be a git repository",
        "returned error: 404",
    ];
    let denied = [
        "fatal: authentication failed",
        "could not read username",
        "returned error: 401",
        "returned error: 403",
    ];
    if not_found.iter().any(|phrase| text.contains(phrase)) {
        ReadError::NotFound
    } else if denied.iter().any(|phrase| text.contains(phrase)) {
        ReadError::AccessDenied
    } else {
        ReadError::Transport
    }
}

impl SourceRefReader for RunnerRefReader {
    fn read_advertisement(&self, _repository_id: &str) -> Result<String, ReadError> {
        let mut invocation =
            GitInvocation::ls_remote(&self.location).map_err(|_| ReadError::Unsupported)?;
        if let Some(auth) = &self.auth {
            invocation = invocation.with_auth(auth.clone());
        }
        let out = self
            .runner
            .run(
                &invocation,
                &WorkDir::Neutral,
                self.limits,
                &self.cancel,
                &[],
            )
            .map_err(|error| match error {
                RunError::Cancelled => ReadError::Cancelled,
                _ => ReadError::Transport,
            })?;
        match out.success_stdout() {
            Ok(text) => Ok(text.to_owned()),
            Err(RunError::OutputIncomplete) if out.stdout_truncated => Err(ReadError::Truncated),
            Err(RunError::NotUtf8) => Err(ReadError::Unsupported),
            Err(RunError::Failed(Termination::Cancelled)) => Err(ReadError::Cancelled),
            Err(RunError::Failed(Termination::Exited(_))) => Err(classify(&out.stderr)),
            Err(_) => Err(ReadError::Transport),
        }
    }
}

#[cfg(test)]
mod tests;
