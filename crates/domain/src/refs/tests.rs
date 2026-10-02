use super::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn oid(fill: char) -> String {
    fill.to_string().repeat(40)
}

fn name(text: &str) -> Result<RefName, String> {
    RefName::parse(text).ok_or_else(|| format!("invalid {text}"))
}

fn advert(lines: &[String]) -> Result<Advertisement, RefError> {
    let mut text = lines.join("\n");
    if !text.is_empty() {
        text.push('\n');
    }
    parse_ls_remote(&text)
}

#[test]
fn ref_names_follow_check_ref_format() {
    for good in [
        "refs/heads/main",
        "refs/heads/release/1.x",
        "refs/heads/odd/naïve-ü",
        "refs/tags/v1.0",
        "refs/tags/a@b",
    ] {
        assert!(RefName::parse(good).is_some(), "{good:?}");
    }
    for bad in [
        "HEAD",
        "refs/heads/",
        "refs/heads/../escape",
        "refs/heads/-option",
        "refs/heads/a..b",
        "refs/heads/with space",
        "refs/heads/trailing.lock",
        "refs/heads/ctrl\u{7}",
        "refs/heads/line\nbreak",
        "refs/heads/@{upstream}",
        "refs/heads/@",
        "refs/heads/a//b",
        "refs/heads/a/",
        "refs/heads/.hidden",
        "refs/heads/dot.",
        "refs/heads/a:b",
        "refs/heads/a~1",
        "refs/heads/a^",
        "refs/heads/a?",
        "refs/heads/a*",
        "refs/heads/a[",
        "refs/heads/a\\b",
        "refs/pull/1/head",
        "refs/remotes/origin/main",
        "refs/notes/commits",
    ] {
        assert!(RefName::parse(bad).is_none(), "{bad:?}");
    }
}

#[test]
fn object_ids_are_full_lowercase_hex() {
    assert!(ObjectId::parse(&oid('a')).is_some());
    assert_eq!(
        ObjectId::parse(&"b".repeat(64)).map(|o| o.format()),
        Some(ObjectFormat::Sha256)
    );
    for bad in [
        "a".repeat(39),
        "a".repeat(41),
        "A".repeat(40),
        "g".repeat(40),
        "0".repeat(40),
        String::new(),
    ] {
        assert!(ObjectId::parse(&bad).is_none(), "{bad:?}");
    }
}

#[test]
fn peeled_tags_and_symbolic_head_are_not_extra_refs() -> TestResult {
    // TEST-REFS (second clause).
    let ad = advert(&[
        "ref: refs/heads/main\tHEAD".to_owned(),
        format!("{}\tHEAD", oid('1')),
        format!("{}\trefs/heads/main", oid('1')),
        format!("{}\trefs/tags/v1.0", oid('2')),
        format!("{}\trefs/tags/v1.0^{{}}", oid('1')),
        format!("{}\trefs/pull/7/head", oid('3')),
        format!("{}\trefs/merge-requests/1/head", oid('3')),
    ])?;
    assert_eq!(ad.refs.len(), 2);
    assert_eq!(ad.head_target.as_deref(), Some("refs/heads/main"));
    // The annotated tag keeps its tag-object ID; the peeled commit is display-only.
    assert_eq!(
        ad.refs.get(&name("refs/tags/v1.0")?).map(ObjectId::as_str),
        Some(oid('2').as_str())
    );
    assert_eq!(
        ad.peeled
            .get(&name("refs/tags/v1.0")?)
            .map(ObjectId::as_str),
        Some(oid('1').as_str())
    );
    assert_eq!(ad.ignored, 2);
    Ok(())
}

#[test]
fn non_default_branch_and_moved_annotated_tag_are_detected() -> TestResult {
    // TEST-REFS: HEAD/default branch and every "metadata" hint unchanged, yet a
    // non-default branch and an annotated tag moved; both must appear in the diff.
    let before = advert(&[
        "ref: refs/heads/main\tHEAD".to_owned(),
        format!("{}\trefs/heads/main", oid('1')),
        format!("{}\trefs/heads/feature", oid('2')),
        format!("{}\trefs/tags/v1", oid('3')),
        format!("{}\trefs/tags/v1^{{}}", oid('1')),
        format!("{}\trefs/tags/gone", oid('4')),
    ])?;
    let after = advert(&[
        "ref: refs/heads/main\tHEAD".to_owned(),
        format!("{}\trefs/heads/main", oid('1')),
        format!("{}\trefs/heads/feature", oid('5')),
        format!("{}\trefs/heads/new", oid('1')),
        format!("{}\trefs/tags/v1", oid('6')),
        format!("{}\trefs/tags/v1^{{}}", oid('1')),
    ])?;
    let changes = diff(&before.refs, &after.refs)?;
    let summary: Vec<(String, Option<String>, Option<String>)> = changes
        .iter()
        .map(|c| {
            (
                c.name.to_string(),
                c.old.as_ref().map(ToString::to_string),
                c.new.as_ref().map(ToString::to_string),
            )
        })
        .collect();
    assert_eq!(
        summary,
        vec![
            ("refs/heads/feature".into(), Some(oid('2')), Some(oid('5'))),
            ("refs/heads/new".into(), None, Some(oid('1'))),
            ("refs/tags/gone".into(), Some(oid('4')), None),
            ("refs/tags/v1".into(), Some(oid('3')), Some(oid('6'))),
        ]
    );
    assert!(diff(&after.refs, &after.refs)?.is_empty());
    Ok(())
}

#[test]
fn malformed_advertisements_fail_closed() {
    let cases: Vec<(Vec<String>, RefError)> = vec![
        (
            vec!["no-tab-here".into()],
            RefError::MalformedLine { line: 1 },
        ),
        (
            vec![format!("{}\trefs/heads/a b", oid('1'))],
            RefError::InvalidRefName { line: 1 },
        ),
        (
            vec![format!("{}\trefs/heads/x", "1".repeat(39))],
            RefError::InvalidObjectId { line: 1 },
        ),
        (
            vec![
                format!("{}\trefs/heads/x", oid('1')),
                format!("{}\trefs/heads/x", oid('2')),
            ],
            RefError::DuplicateRef { line: 2 },
        ),
        (
            vec![
                format!("{}\trefs/heads/x", oid('1')),
                format!("{}\trefs/heads/y", "2".repeat(64)),
            ],
            RefError::MixedObjectFormat,
        ),
        (
            vec![format!("{}\trefs/tags/v^{{}}", oid('1'))],
            RefError::OrphanPeeled { line: 1 },
        ),
        (
            vec![
                format!("{}\trefs/tags/a", oid('1')),
                format!("{}\trefs/tags/b^{{}}", oid('1')),
            ],
            RefError::OrphanPeeled { line: 2 },
        ),
        (
            vec![
                "ref: refs/heads/main\tHEAD".into(),
                "ref: refs/heads/dev\tHEAD".into(),
            ],
            RefError::MalformedLine { line: 2 },
        ),
        (
            vec![
                format!("{}\trefs/custom/x", oid('1')),
                format!("{}\trefs/custom/y^{{}}", oid('2')),
            ],
            RefError::OrphanPeeled { line: 2 },
        ),
    ];
    for (lines, expected) in cases {
        assert_eq!(advert(&lines), Err(expected.clone()), "{lines:?}");
    }
}

#[test]
fn empty_repository_is_an_explicit_empty_map() -> TestResult {
    // TEST-EMPTY-REPO (domain): no refs, possibly an unborn HEAD target; no invented
    // HEAD ref and no default-branch OID.
    let none = parse_ls_remote("")?;
    assert!(none.refs.is_empty());
    assert_eq!(none.head_target, None);
    let unborn = parse_ls_remote("ref: refs/heads/main\tHEAD\n")?;
    assert!(unborn.refs.is_empty());
    assert_eq!(unborn.head_target.as_deref(), Some("refs/heads/main"));
    assert_eq!(unborn.refs.format(), None);
    // First commit later is a detected change against the empty map.
    let first = parse_ls_remote(&format!("{}\trefs/heads/main\n", oid('a')))?;
    assert_eq!(diff(&unborn.refs, &first.refs)?.len(), 1);
    Ok(())
}

#[test]
fn diff_refuses_maps_of_different_object_formats() -> TestResult {
    let sha1 = RefMap::from_entries([(
        name("refs/heads/m")?,
        ObjectId::parse(&oid('1')).ok_or("oid")?,
    )])?;
    let sha256 = RefMap::from_entries([(
        name("refs/heads/m")?,
        ObjectId::parse(&"1".repeat(64)).ok_or("oid")?,
    )])?;
    assert_eq!(diff(&sha1, &sha256), Err(RefError::MixedObjectFormat));
    Ok(())
}

#[test]
fn v2_symrefs_and_peeled_entries_of_ignored_refs_are_accepted() -> TestResult {
    // Review M1/M2: real v2 output for a symbolic branch, a remote-tracking HEAD, an
    // annotated tag under a custom namespace and a detached HEAD at an annotated tag.
    let ad = advert(&[
        format!("{}\tHEAD", oid('2')),
        format!("{}\tHEAD^{{}}", oid('1')),
        "ref: refs/heads/main\trefs/heads/alias".to_owned(),
        format!("{}\trefs/heads/alias", oid('1')),
        format!("{}\trefs/heads/main", oid('1')),
        format!("{}\trefs/custom/x", oid('3')),
        format!("{}\trefs/custom/x^{{}}", oid('1')),
        "ref: refs/remotes/origin/HEAD\trefs/remotes/origin/HEAD".to_owned(),
        format!("{}\trefs/remotes/origin/HEAD", oid('1')),
    ])?;
    assert_eq!(ad.refs.len(), 2, "alias and main are both real branch refs");
    assert_eq!(ad.head_target, None, "detached HEAD has no symref target");
    assert!(ad.peeled.is_empty());
    assert_eq!(ad.ignored, 2);
    Ok(())
}

#[test]
fn output_cut_mid_line_is_truncated_not_a_shorter_map() {
    // Review L1: git always terminates lines; a missing final newline means cut short.
    let cut = format!("{}\trefs/heads/main\n{}\trefs/heads/ma", oid('1'), oid('2'));
    assert_eq!(parse_ls_remote(&cut), Err(RefError::Truncated));
    assert_eq!(
        parse_ls_remote(&format!("{}\trefs/heads/main", oid('1'))),
        Err(RefError::Truncated)
    );
}
