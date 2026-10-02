use std::collections::HashSet;

use super::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn oid(fill: char) -> Result<ObjectId, String> {
    ObjectId::parse(&fill.to_string().repeat(40)).ok_or_else(|| format!("oid {fill}"))
}

fn map(entries: &[(&str, char)]) -> Result<RefMap, Box<dyn std::error::Error>> {
    let mut out = Vec::new();
    for (name, fill) in entries {
        out.push((RefName::parse(name).ok_or("name")?, oid(*fill)?));
    }
    Ok(RefMap::from_entries(out)?)
}

/// Ancestry facts as (ancestor, descendant) pairs; anything else is "not ancestor",
/// and pairs in `unknown` cannot be determined.
struct Graph {
    edges: HashSet<(char, char)>,
    unknown: HashSet<(char, char)>,
}

impl Graph {
    fn new(edges: &[(char, char)]) -> Self {
        Self {
            edges: edges.iter().copied().collect(),
            unknown: HashSet::new(),
        }
    }
}

impl AncestryOracle for Graph {
    fn is_ancestor(&self, a: &ObjectId, d: &ObjectId) -> Option<bool> {
        let key = (a.as_str().chars().next()?, d.as_str().chars().next()?);
        if self.unknown.contains(&key) {
            return None;
        }
        Some(key.0 == key.1 || self.edges.contains(&key))
    }
}

fn kinds(classification: &Classification) -> Vec<(String, OperationKind)> {
    match classification {
        Classification::Operations(ops) => ops
            .iter()
            .map(|op| (op.name.to_string(), op.kind))
            .collect(),
        Classification::DestinationDrift(_) => Vec::new(),
    }
}

#[test]
fn every_operation_class_is_derived_from_state_and_ancestry() -> TestResult {
    let verified = map(&[
        ("refs/heads/main", 'a'),
        ("refs/heads/rewritten", 'b'),
        ("refs/heads/gone", 'c'),
        ("refs/tags/v1", 'd'),
        ("refs/tags/same", 'e'),
    ])?;
    let source = map(&[
        ("refs/heads/main", '1'),      // a -> 1 descendant
        ("refs/heads/rewritten", '2'), // b -> 2 unrelated
        ("refs/heads/new", '3'),
        ("refs/tags/v1", '4'), // tag moved
        ("refs/tags/same", 'e'),
    ])?;
    let result = classify(
        &source,
        &verified,
        Some(&verified),
        &Graph::new(&[('a', '1')]),
    )?;
    assert_eq!(
        kinds(&result),
        vec![
            ("refs/heads/gone".to_owned(), OperationKind::DeleteRef),
            ("refs/heads/main".to_owned(), OperationKind::FastForward),
            ("refs/heads/new".to_owned(), OperationKind::Create),
            ("refs/heads/rewritten".to_owned(), OperationKind::Rewrite),
            ("refs/tags/v1".to_owned(), OperationKind::MoveTag),
        ]
    );
    // Leases: the exact current destination value (absence for a create).
    let Classification::Operations(ops) = result else {
        return Err("drift".into());
    };
    for op in &ops {
        assert_eq!(op.expected_destination.as_ref(), verified.get(&op.name));
        assert_eq!(op.source.as_ref(), source.get(&op.name));
    }
    Ok(())
}

#[test]
fn unknown_ancestry_is_a_rewrite_never_a_fast_forward() -> TestResult {
    let verified = map(&[("refs/heads/main", 'a')])?;
    let source = map(&[("refs/heads/main", '1')])?;
    let mut graph = Graph::new(&[('a', '1')]);
    graph.unknown.insert(('a', '1'));
    let result = classify(&source, &verified, Some(&verified), &graph)?;
    assert_eq!(
        kinds(&result),
        vec![("refs/heads/main".to_owned(), OperationKind::Rewrite)]
    );
    Ok(())
}

#[test]
fn tags_are_never_fast_forwarded() -> TestResult {
    let verified = map(&[("refs/tags/v1", 'a')])?;
    let source = map(&[("refs/tags/v1", '1')])?;
    let result = classify(
        &source,
        &verified,
        Some(&verified),
        &Graph::new(&[('a', '1')]),
    )?;
    assert_eq!(
        kinds(&result),
        vec![("refs/tags/v1".to_owned(), OperationKind::MoveTag)]
    );
    Ok(())
}

#[test]
fn any_destination_change_outside_the_app_blocks_the_whole_plan() -> TestResult {
    let verified = map(&[("refs/heads/main", 'a'), ("refs/heads/x", 'b')])?;
    let source = map(&[("refs/heads/main", '1'), ("refs/heads/x", 'b')])?;
    let graph = Graph::new(&[('a', '1')]);
    for destination in [
        // Someone pushed to main.
        map(&[("refs/heads/main", 'f'), ("refs/heads/x", 'b')])?,
        // Someone deleted x.
        map(&[("refs/heads/main", 'a')])?,
        // Someone added a branch the app does not know.
        map(&[
            ("refs/heads/main", 'a'),
            ("refs/heads/x", 'b'),
            ("refs/heads/theirs", 'c'),
        ])?,
    ] {
        let result = classify(&source, &destination, Some(&verified), &graph)?;
        let Classification::DestinationDrift(drift) = result else {
            return Err(format!("expected drift for {destination:?}").into());
        };
        assert_eq!(drift.len(), 1);
    }
    Ok(())
}

#[test]
fn a_never_written_destination_must_be_empty() -> TestResult {
    let source = map(&[("refs/heads/main", '1')])?;
    let result = classify(&source, &RefMap::default(), None, &Graph::new(&[]))?;
    assert_eq!(
        kinds(&result),
        vec![("refs/heads/main".to_owned(), OperationKind::Create)]
    );
    // Same-named content already there is not ours: drift, not an overwrite.
    let occupied = map(&[("refs/heads/main", '1')])?;
    assert!(matches!(
        classify(&source, &occupied, None, &Graph::new(&[]))?,
        Classification::DestinationDrift(_)
    ));
    Ok(())
}

#[test]
fn only_refs_the_app_verified_can_be_deleted() -> TestResult {
    // An empty (but complete) source: every managed ref is a reviewed delete, nothing
    // else is touched.
    let verified = map(&[("refs/heads/main", 'a'), ("refs/tags/v1", 'b')])?;
    let result = classify(
        &RefMap::default(),
        &verified,
        Some(&verified),
        &Graph::new(&[]),
    )?;
    let all = kinds(&result);
    assert_eq!(all.len(), 2);
    assert!(
        all.iter()
            .all(|(_, kind)| *kind == OperationKind::DeleteRef)
    );
    assert!(all.iter().all(|(_, kind)| kind.is_destructive()));
    Ok(())
}

#[test]
fn equal_state_needs_no_operations_and_formats_must_agree() -> TestResult {
    let refs = map(&[("refs/heads/main", 'a')])?;
    assert_eq!(
        classify(&refs, &refs, Some(&refs), &Graph::new(&[]))?,
        Classification::Operations(Vec::new())
    );
    let sha256 = RefMap::from_entries([(
        RefName::parse("refs/heads/main").ok_or("name")?,
        ObjectId::parse(&"a".repeat(64)).ok_or("oid")?,
    )])?;
    assert_eq!(
        classify(&sha256, &refs, Some(&refs), &Graph::new(&[])),
        Err(RefError::MixedObjectFormat)
    );
    Ok(())
}

#[test]
fn destructive_kinds_are_exactly_rewrite_move_and_delete() {
    use OperationKind::*;
    for (kind, destructive) in [
        (Create, false),
        (FastForward, false),
        (Rewrite, true),
        (MoveTag, true),
        (DeleteRef, true),
    ] {
        assert_eq!(kind.is_destructive(), destructive, "{kind:?}");
    }
}
