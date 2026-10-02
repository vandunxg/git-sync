//! Capture logic against an in-memory mirror and source (test-only fakes; the real
//! Git adapter is exercised in infrastructure/src/git/mirror tests).

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::sync::Mutex;

use super::*;
use crate::observation::ReadError;

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Hex fill digit -> OID; an invalid fill (e.g. all zeros) falls back to `a...a`.
fn oid(fill: char) -> ObjectId {
    match ObjectId::parse(&fill.to_string().repeat(40)) {
        Some(value) => value,
        None => oid('a'),
    }
}

/// Shared source state read by `FakeReader` and fetched by `FakeMirror`.
#[derive(Default)]
struct Source {
    refs: Mutex<BTreeMap<String, ObjectId>>,
    unavailable: Mutex<bool>,
}

impl Source {
    fn set(&self, name: &str, value: Option<ObjectId>) {
        if let Ok(mut refs) = self.refs.lock() {
            match value {
                Some(oid) => refs.insert(name.to_owned(), oid),
                None => refs.remove(name),
            };
        }
    }

    fn snapshot(&self) -> BTreeMap<String, ObjectId> {
        self.refs.lock().map(|r| r.clone()).unwrap_or_default()
    }
}

struct FakeReader<'a>(&'a Source);

impl SourceRefReader for FakeReader<'_> {
    fn read_advertisement(&self, _: &str) -> Result<String, ReadError> {
        if self.0.unavailable.lock().map(|u| *u).unwrap_or(true) {
            return Err(ReadError::NotFound);
        }
        let mut text = String::from("ref: refs/heads/main\tHEAD\n");
        for (name, oid) in self.0.snapshot() {
            text.push_str(&format!("{oid}\t{name}\n"));
        }
        Ok(text)
    }
}

struct Clock(Cell<u64>);

impl ObservationClock for Clock {
    fn next(&self) -> (u64, i64) {
        let n = self.0.get() + 1;
        self.0.set(n);
        (n, i64::try_from(n).unwrap_or(0) * 1000)
    }
}

type Hook<'a> = Box<dyn Fn(usize, bool, &Source) + 'a>;

#[derive(Default)]
struct FakeMirror<'a> {
    source: Option<&'a Source>,
    refs: RefCell<BTreeMap<String, ObjectId>>,
    prepared: Cell<bool>,
    fetches: Cell<usize>,
    /// Called with (fetch number, before?) around each fetch to move the source:
    /// `true` = between observation and transfer, `false` = right after transfer.
    hook: Option<Hook<'a>>,
    /// Corrupt one staged ref after fetch although "Git" succeeded.
    lie: Cell<bool>,
    integrity_ok: Cell<bool>,
    /// Copy only this many refs, then fail (non-atomic partial promotion).
    copy_fails_after: Cell<Option<usize>>,
    /// Fail this many source fetches (e.g. a ref deleted after observation).
    fetch_failures: Cell<usize>,
    fetch_error: Cell<Option<MirrorError>>,
}

impl<'a> FakeMirror<'a> {
    fn new(source: &'a Source) -> Self {
        Self {
            source: Some(source),
            integrity_ok: Cell::new(true),
            ..Self::default()
        }
    }
}

impl MirrorGit for FakeMirror<'_> {
    fn prepare(&self, _: ObjectFormat) -> Result<(), MirrorError> {
        self.prepared.set(true);
        Ok(())
    }

    fn fetch_from_source(&self, pairs: &[(RefName, String)]) -> Result<(), MirrorError> {
        let source = self.source.ok_or(MirrorError::Failed)?;
        if let Some(error) = self.fetch_error.get() {
            return Err(error);
        }
        if self.fetch_failures.get() > 0 {
            self.fetch_failures.set(self.fetch_failures.get() - 1);
            self.fetches.set(self.fetches.get() + 1);
            return Err(MirrorError::Failed);
        }
        let n = self.fetches.get() + 1;
        self.fetches.set(n);
        if let Some(hook) = &self.hook {
            hook(n, true, source);
        }
        // Real fetch transfers the source's *current* tips.
        let current = source.snapshot();
        let mut refs = self.refs.borrow_mut();
        for (from, to) in pairs {
            if let Some(oid) = current.get(from.as_str()) {
                refs.insert(to.clone(), oid.clone());
            }
        }
        if self.lie.get()
            && let Some((_, value)) = refs.iter_mut().find(|(k, _)| k.contains("/staging/"))
        {
            *value = oid('e');
        }
        drop(refs);
        if let Some(hook) = &self.hook {
            hook(n, false, source);
        }
        Ok(())
    }

    fn copy_refs(&self, pairs: &[(String, String)]) -> Result<(), MirrorError> {
        let mut refs = self.refs.borrow_mut();
        for (index, (from, to)) in pairs.iter().enumerate() {
            if self.copy_fails_after.get() == Some(index) {
                return Err(MirrorError::Failed);
            }
            let value = refs.get(from).cloned().ok_or(MirrorError::Failed)?;
            refs.insert(to.clone(), value);
        }
        Ok(())
    }

    fn read_refs(&self, prefix: &str) -> Result<Vec<(String, ObjectId)>, MirrorError> {
        Ok(self
            .refs
            .borrow()
            .iter()
            .filter(|(k, _)| k.starts_with(prefix))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect())
    }

    fn check_integrity(&self) -> Result<(), MirrorError> {
        if self.integrity_ok.get() {
            Ok(())
        } else {
            Err(MirrorError::Failed)
        }
    }

    fn remove_namespace(&self, prefix: &str) -> Result<(), MirrorError> {
        self.refs.borrow_mut().retain(|k, _| !k.starts_with(prefix));
        Ok(())
    }

    fn delete_refs(&self, refs: &[(String, ObjectId)]) -> Result<(), MirrorError> {
        let mut map = self.refs.borrow_mut();
        for (name, expected) in refs {
            if map.get(name) == Some(expected) {
                map.remove(name);
            }
        }
        Ok(())
    }
}

fn request<'a>(job: &'a str, snapshot: &'a str) -> CaptureRequest<'a> {
    CaptureRequest {
        repository_id: "repo-1",
        job_id: job,
        snapshot_id: snapshot,
    }
}

fn populated() -> Source {
    let source = Source::default();
    source.set("refs/heads/main", Some(oid('1')));
    source.set("refs/heads/feature", Some(oid('2')));
    source.set("refs/tags/v1", Some(oid('3')));
    source
}

#[test]
fn captures_every_selected_ref_and_cleans_staging() -> TestResult {
    let source = populated();
    let mirror = FakeMirror::new(&source);
    let out = capture_snapshot(
        &FakeReader(&source),
        &mirror,
        &Clock(Cell::new(0)),
        &request("j1", "s1"),
    )?;
    assert_eq!(out.manifest.refs.len(), 3);
    assert_eq!(
        out.manifest.ref_namespace.as_deref(),
        Some("refs/gbm/snapshots/s1/")
    );
    assert_eq!(out.recheck, SourceRecheck::Unchanged);
    assert_eq!(out.manifest.git_refs, Coverage::Complete);
    assert_eq!(out.manifest.lfs, Coverage::NotChecked);
    let refs = mirror.refs.borrow();
    assert!(
        refs.keys().all(|k| k.starts_with("refs/gbm/snapshots/s1/")),
        "{refs:?}"
    );
    assert_eq!(refs.get("refs/gbm/snapshots/s1/tags/v1"), Some(&oid('3')));
    assert_eq!(out.manifest.ref_set_digest.len(), 64);
    Ok(())
}

#[test]
fn empty_repository_gets_an_explicit_empty_manifest_without_fetch() -> TestResult {
    // TEST-EMPTY-REPO: no fetch, no mirror preparation, no fabricated HEAD ref.
    let source = Source::default();
    let mirror = FakeMirror::new(&source);
    let out = capture_snapshot(
        &FakeReader(&source),
        &mirror,
        &Clock(Cell::new(0)),
        &request("j1", "s1"),
    )?;
    assert!(out.manifest.refs.is_empty());
    assert_eq!(out.manifest.object_format, None);
    assert_eq!(out.manifest.ref_namespace, None);
    assert_eq!(out.manifest.head_target.as_deref(), Some("refs/heads/main"));
    assert_eq!(mirror.fetches.get(), 0);
    assert!(!mirror.prepared.get());
    // The first commit later is detected and captured.
    source.set("refs/heads/main", Some(oid('9')));
    let next = capture_snapshot(
        &FakeReader(&source),
        &mirror,
        &Clock(Cell::new(10)),
        &request("j2", "s2"),
    )?;
    assert_eq!(next.manifest.refs.len(), 1);
    assert_ne!(next.manifest.ref_set_digest, out.manifest.ref_set_digest);
    Ok(())
}

#[test]
fn a_source_move_during_fetch_is_captured_coherently_never_mixed() -> TestResult {
    // The source moves between observation and transfer: staged != first observation,
    // but equals the re-observation, which is what gets recorded (one fetch).
    let source = populated();
    let mut mirror = FakeMirror::new(&source);
    mirror.hook = Some(Box::new(|n, before, s: &Source| {
        if n == 1 && before {
            s.set("refs/heads/feature", Some(oid('7')));
        }
    }));
    let out = capture_snapshot(
        &FakeReader(&source),
        &mirror,
        &Clock(Cell::new(0)),
        &request("j1", "s1"),
    )?;
    assert_eq!(mirror.fetches.get(), 1);
    let feature = RefName::parse("refs/heads/feature").ok_or("name")?;
    assert_eq!(out.manifest.refs.get(&feature), Some(&oid('7')));
    assert_eq!(out.recheck, SourceRecheck::Unchanged);
    Ok(())
}

#[test]
fn a_move_right_after_transfer_records_the_observed_map_and_reports_changed() -> TestResult {
    let source = populated();
    let mut mirror = FakeMirror::new(&source);
    mirror.hook = Some(Box::new(|n, before, s: &Source| {
        if n == 1 && !before {
            s.set("refs/heads/main", Some(oid('6')));
        }
    }));
    let out = capture_snapshot(
        &FakeReader(&source),
        &mirror,
        &Clock(Cell::new(0)),
        &request("j1", "s1"),
    )?;
    let main = RefName::parse("refs/heads/main").ok_or("name")?;
    assert_eq!(
        out.manifest.refs.get(&main),
        Some(&oid('1')),
        "coherent older map"
    );
    assert_eq!(
        out.recheck,
        SourceRecheck::Changed,
        "newer source is an update"
    );
    Ok(())
}

#[test]
fn deleted_source_ref_between_attempts_does_not_linger_in_staging() -> TestResult {
    // Main moves before transfer 1 (staged != observation 1) and v1 is deleted right
    // after it (re-observation != staged), so attempt 2 runs; the v1 staged by attempt 1
    // must not be carried forward.
    let source = populated();
    let mut mirror = FakeMirror::new(&source);
    mirror.hook = Some(Box::new(|n, before, s: &Source| {
        if n == 1 && before {
            s.set("refs/heads/main", Some(oid('4')));
        }
        if n == 1 && !before {
            s.set("refs/tags/v1", None);
        }
    }));
    let out = capture_snapshot(
        &FakeReader(&source),
        &mirror,
        &Clock(Cell::new(0)),
        &request("j1", "s1"),
    )?;
    assert_eq!(mirror.fetches.get(), 2);
    assert_eq!(out.manifest.refs.len(), 2);
    assert!(
        !mirror.refs.borrow().keys().any(|k| k.ends_with("tags/v1")),
        "{:?}",
        mirror.refs.borrow()
    );
    Ok(())
}

#[test]
fn git_success_with_a_wrong_ref_is_never_promoted() {
    // TEST-VERIFY: "Git exits 0 but a ref differs afterward".
    let source = populated();
    let mirror = FakeMirror::new(&source);
    mirror.lie.set(true);
    let out = capture_snapshot(
        &FakeReader(&source),
        &mirror,
        &Clock(Cell::new(0)),
        &request("j1", "s1"),
    );
    assert_eq!(out, Err(CaptureError::SourceKeptChanging));
    assert!(
        mirror.refs.borrow().is_empty(),
        "no snapshot refs, staging cleaned"
    );
}

#[test]
fn a_source_that_keeps_moving_exhausts_the_budget() {
    // Moves before every transfer (staged != observation) and again after it
    // (re-observation != staged): no coherent map ever exists.
    let source = populated();
    let mut mirror = FakeMirror::new(&source);
    mirror.hook = Some(Box::new(|n, before, s: &Source| {
        let base = u32::try_from(n % 4).unwrap_or(0) * 2 + if before { 1 } else { 2 };
        let fill = char::from_digit(base, 10).unwrap_or('1');
        s.set("refs/heads/main", Some(oid(fill)));
    }));
    let out = capture_snapshot(
        &FakeReader(&source),
        &mirror,
        &Clock(Cell::new(0)),
        &request("j1", "s1"),
    );
    assert_eq!(out, Err(CaptureError::SourceKeptChanging));
    assert_eq!(mirror.fetches.get(), MAX_CAPTURE_ATTEMPTS);
    assert!(mirror.refs.borrow().is_empty());
}

#[test]
fn newer_source_after_capture_is_reported_changed() -> TestResult {
    // TEST-VERIFY: "source changes again during verification" -> update, not synced.
    let source = populated();
    let mirror = FakeMirror::new(&source);
    struct MovingReader<'a>(&'a Source, std::sync::atomic::AtomicUsize);
    impl SourceRefReader for MovingReader<'_> {
        fn read_advertisement(&self, id: &str) -> Result<String, ReadError> {
            let n = self.1.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            if n == 2 {
                self.0.set("refs/heads/main", Some(oid('8')));
            }
            FakeReader(self.0).read_advertisement(id)
        }
    }
    let reader = MovingReader(&source, std::sync::atomic::AtomicUsize::new(0));
    let out = capture_snapshot(&reader, &mirror, &Clock(Cell::new(0)), &request("j1", "s1"))?;
    let main = RefName::parse("refs/heads/main").ok_or("name")?;
    assert_eq!(out.manifest.refs.get(&main), Some(&oid('1')));
    assert_eq!(out.recheck, SourceRecheck::Changed);
    Ok(())
}

#[test]
fn unavailable_source_or_bad_ids_touch_nothing() {
    let source = populated();
    if let Ok(mut u) = source.unavailable.lock() {
        *u = true;
    }
    let mirror = FakeMirror::new(&source);
    let out = capture_snapshot(
        &FakeReader(&source),
        &mirror,
        &Clock(Cell::new(0)),
        &request("j1", "s1"),
    );
    assert_eq!(
        out,
        Err(CaptureError::SourceUnavailable(UnavailableReason::NotFound))
    );
    assert!(!mirror.prepared.get() && mirror.fetches.get() == 0);
    for (job, snap) in [
        ("../x", "s1"),
        ("j1", "a/b"),
        ("", "s1"),
        ("-j", "s1"),
        ("j.lock", "s1"),
    ] {
        assert_eq!(
            capture_snapshot(
                &FakeReader(&source),
                &mirror,
                &Clock(Cell::new(0)),
                &request(job, snap)
            ),
            Err(CaptureError::InvalidId),
            "{job} {snap}"
        );
    }
}

#[test]
fn integrity_failure_and_existing_snapshot_block_promotion() -> TestResult {
    let source = populated();
    let mirror = FakeMirror::new(&source);
    mirror.integrity_ok.set(false);
    let out = capture_snapshot(
        &FakeReader(&source),
        &mirror,
        &Clock(Cell::new(0)),
        &request("j1", "s1"),
    );
    assert_eq!(out, Err(CaptureError::IntegrityFailed));
    assert!(mirror.refs.borrow().is_empty());

    mirror.integrity_ok.set(true);
    capture_snapshot(
        &FakeReader(&source),
        &mirror,
        &Clock(Cell::new(0)),
        &request("j1", "s1"),
    )?;
    let before = mirror.refs.borrow().clone();
    source.set("refs/heads/main", Some(oid('5')));
    let again = capture_snapshot(
        &FakeReader(&source),
        &mirror,
        &Clock(Cell::new(9)),
        &request("j2", "s1"),
    );
    assert_eq!(again, Err(CaptureError::SnapshotExists));
    assert_eq!(*mirror.refs.borrow(), before, "previous snapshot untouched");
    Ok(())
}

#[test]
fn digest_covers_names_oids_and_format_only() -> TestResult {
    let a = RefMap::from_entries([(RefName::parse("refs/heads/main").ok_or("n")?, oid('1'))])?;
    let b = RefMap::from_entries([(RefName::parse("refs/heads/main").ok_or("n")?, oid('2'))])?;
    let c = RefMap::from_entries([(RefName::parse("refs/heads/other").ok_or("n")?, oid('1'))])?;
    assert_ne!(ref_set_digest(&a), ref_set_digest(&b));
    assert_ne!(ref_set_digest(&a), ref_set_digest(&c));
    assert_eq!(ref_set_digest(&a), ref_set_digest(&a.clone()));
    assert_ne!(ref_set_digest(&RefMap::default()), ref_set_digest(&a));
    Ok(())
}

#[test]
fn a_partial_promotion_is_removed_and_the_id_stays_usable() -> TestResult {
    // Review #1: copy fails after one ref; no partial snapshot namespace may remain.
    let source = populated();
    let mirror = FakeMirror::new(&source);
    mirror.copy_fails_after.set(Some(1));
    let out = capture_snapshot(
        &FakeReader(&source),
        &mirror,
        &Clock(Cell::new(0)),
        &request("j1", "s1"),
    );
    assert_eq!(out, Err(CaptureError::Mirror(MirrorError::Failed)));
    assert!(
        mirror.refs.borrow().is_empty(),
        "{:?}",
        mirror.refs.borrow()
    );
    mirror.copy_fails_after.set(None);
    let retry = capture_snapshot(
        &FakeReader(&source),
        &mirror,
        &Clock(Cell::new(5)),
        &request("j2", "s1"),
    )?;
    assert_eq!(retry.manifest.refs.len(), 3);
    Ok(())
}

#[test]
fn cancellation_is_reported_as_cancelled_and_staging_is_removed() {
    // Review #2: never "source unavailable"; cleanup still runs.
    struct CancelledReader;
    impl SourceRefReader for CancelledReader {
        fn read_advertisement(&self, _: &str) -> Result<String, ReadError> {
            Err(ReadError::Cancelled)
        }
    }
    let source = populated();
    let mirror = FakeMirror::new(&source);
    let out = capture_snapshot(
        &CancelledReader,
        &mirror,
        &Clock(Cell::new(0)),
        &request("j1", "s1"),
    );
    assert_eq!(out, Err(CaptureError::Cancelled));

    mirror
        .refs
        .borrow_mut()
        .insert("refs/gbm/staging/j1/heads/old".to_owned(), oid('4'));
    mirror.fetch_error.set(Some(MirrorError::Cancelled));
    let out = capture_snapshot(
        &FakeReader(&source),
        &mirror,
        &Clock(Cell::new(0)),
        &request("j1", "s1"),
    );
    assert_eq!(out, Err(CaptureError::Cancelled));
    assert!(
        mirror.refs.borrow().is_empty(),
        "staging removed after cancel"
    );
}

#[test]
fn a_failed_fetch_is_retried_after_re_observing() -> TestResult {
    // Review #3: e.g. "couldn't find remote ref" for a ref deleted after observation.
    let source = populated();
    let mirror = FakeMirror::new(&source);
    mirror.fetch_failures.set(1);
    let out = capture_snapshot(
        &FakeReader(&source),
        &mirror,
        &Clock(Cell::new(0)),
        &request("j1", "s1"),
    )?;
    assert_eq!(out.manifest.refs.len(), 3);
    assert_eq!(mirror.fetches.get(), 2);
    // Persistent fetch failure surfaces as the mirror error, not as "kept changing".
    let failing = FakeMirror::new(&source);
    failing.fetch_failures.set(MAX_CAPTURE_ATTEMPTS);
    assert_eq!(
        capture_snapshot(
            &FakeReader(&source),
            &failing,
            &Clock(Cell::new(0)),
            &request("j2", "s2")
        ),
        Err(CaptureError::Mirror(MirrorError::Failed))
    );
    Ok(())
}
