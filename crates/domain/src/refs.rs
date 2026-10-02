//! Selected ref maps and their comparison (REQ-07; SYNC_ENGINE.md "Ref comparison").
//!
//! A ref map holds full logical names (`refs/heads/*`, `refs/tags/*`) and full object
//! IDs. Annotated tags keep their tag-object ID; the peeled `^{}` entries and the
//! symbolic `HEAD` are recorded separately and never take part in comparison. Parsing
//! fails closed: one malformed line, invalid selected name, duplicate or mixed object
//! format rejects the whole advertisement, so a partial map is never mistaken for a
//! complete one.

use std::collections::BTreeMap;
use std::fmt;

/// Upper bound for one ref name (Git has no hard limit; real names are far shorter).
const MAX_REF_NAME: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefError {
    MalformedLine {
        line: usize,
    },
    InvalidObjectId {
        line: usize,
    },
    InvalidRefName {
        line: usize,
    },
    DuplicateRef {
        line: usize,
    },
    /// SHA-1 and SHA-256 IDs in one advertisement, or two maps of different formats.
    MixedObjectFormat,
    /// A `^{}` entry without its tag directly before it.
    OrphanPeeled {
        line: usize,
    },
    /// Non-empty output not ending in a newline (cut short).
    Truncated,
}

impl fmt::Display for RefError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MalformedLine { line } => write!(f, "malformed ref line {line}"),
            Self::InvalidObjectId { line } => write!(f, "invalid object id on line {line}"),
            Self::InvalidRefName { line } => write!(f, "unsupported ref name on line {line}"),
            Self::DuplicateRef { line } => write!(f, "duplicate ref on line {line}"),
            Self::MixedObjectFormat => f.write_str("mixed object formats"),
            Self::OrphanPeeled { line } => write!(f, "peeled entry without tag on line {line}"),
            Self::Truncated => f.write_str("ref listing was cut short"),
        }
    }
}

impl std::error::Error for RefError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectFormat {
    Sha1,
    Sha256,
}

/// A full lower-case hex object ID (40 for SHA-1, 64 for SHA-256). The all-zero ID is
/// Git's "absent" marker and is never a valid observed value.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjectId(String);

impl ObjectId {
    pub fn parse(text: &str) -> Option<Self> {
        let sized = matches!(text.len(), 40 | 64);
        let hex = text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
        let zero = text.bytes().all(|b| b == b'0');
        (sized && hex && !zero).then(|| Self(text.to_owned()))
    }

    pub fn format(&self) -> ObjectFormat {
        if self.0.len() == 64 {
            ObjectFormat::Sha256
        } else {
            ObjectFormat::Sha1
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ObjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefKind {
    Branch,
    Tag,
}

/// A selected logical ref name: `refs/heads/<x>` or `refs/tags/<x>` where `<x>`
/// follows `git check-ref-format` and additionally may not start with `-`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RefName(String);

impl RefName {
    pub fn parse(text: &str) -> Option<Self> {
        let rest = text
            .strip_prefix("refs/heads/")
            .or_else(|| text.strip_prefix("refs/tags/"))?;
        (text.len() <= MAX_REF_NAME && is_valid_short_name(rest)).then(|| Self(text.to_owned()))
    }

    pub fn kind(&self) -> RefKind {
        if self.0.starts_with("refs/heads/") {
            RefKind::Branch
        } else {
            RefKind::Tag
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RefName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// `git check-ref-format` rules for the part after `refs/heads/` or `refs/tags/`.
fn is_valid_short_name(rest: &str) -> bool {
    let forbidden_char = |c: char| {
        c.is_ascii_control() || matches!(c, ' ' | '~' | '^' | ':' | '?' | '*' | '[' | '\\')
    };
    !rest.is_empty()
        && !rest.starts_with('-')
        && !rest.ends_with('.')
        && rest != "@"
        && !rest.contains("..")
        && !rest.contains("@{")
        && !rest.chars().any(forbidden_char)
        && rest
            .split('/')
            .all(|part| !part.is_empty() && !part.starts_with('.') && !part.ends_with(".lock"))
}

/// A complete selected ref map. `format` is `None` only for an empty map.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RefMap {
    refs: BTreeMap<RefName, ObjectId>,
}

impl RefMap {
    pub fn from_entries(
        entries: impl IntoIterator<Item = (RefName, ObjectId)>,
    ) -> Result<Self, RefError> {
        let mut map = Self::default();
        for (index, (name, oid)) in entries.into_iter().enumerate() {
            map.insert(name, oid, index + 1)?;
        }
        Ok(map)
    }

    fn insert(&mut self, name: RefName, oid: ObjectId, line: usize) -> Result<(), RefError> {
        if self.format().is_some_and(|format| format != oid.format()) {
            return Err(RefError::MixedObjectFormat);
        }
        if self.refs.insert(name, oid).is_some() {
            return Err(RefError::DuplicateRef { line });
        }
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.refs.is_empty()
    }

    pub fn len(&self) -> usize {
        self.refs.len()
    }

    pub fn format(&self) -> Option<ObjectFormat> {
        self.refs.values().next().map(ObjectId::format)
    }

    pub fn get(&self, name: &RefName) -> Option<&ObjectId> {
        self.refs.get(name)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&RefName, &ObjectId)> {
        self.refs.iter()
    }
}

/// Parsed `git ls-remote --symref` output.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Advertisement {
    pub refs: RefMap,
    /// Target of the symbolic `HEAD`, when advertised. It may name a branch that does
    /// not exist (unborn default branch of an empty repository); it is never invented.
    pub head_target: Option<String>,
    /// Peeled commit of each annotated tag, for display only (not compared).
    pub peeled: BTreeMap<RefName, ObjectId>,
    /// Entries outside heads/tags (e.g. `refs/pull/*`), ignored by policy.
    pub ignored: usize,
}

/// Parses `git ls-remote --symref` output (protocol v0 or v2). Git terminates every
/// line with `\n`; non-empty text without a final newline is treated as truncated.
pub fn parse_ls_remote(text: &str) -> Result<Advertisement, RefError> {
    if !text.is_empty() && !text.ends_with('\n') {
        return Err(RefError::Truncated);
    }
    let mut out = Advertisement::default();
    // Name of the previous non-peeled entry (any namespace) and, if it was a selected
    // tag, that tag: a `^{}` line must directly follow its base.
    let mut previous_name: Option<&str> = None;
    let mut previous_tag: Option<RefName> = None;
    for (index, line) in text.lines().enumerate() {
        let line_no = index + 1;
        let (left, name) = line
            .split_once('\t')
            .ok_or(RefError::MalformedLine { line: line_no })?;
        if let Some(target) = left.strip_prefix("ref: ") {
            // v2 advertises a target for every symbolic ref; only HEAD's is recorded.
            // A symbolic ref under heads/tags still gets its own `<oid>\t<name>` line.
            if name == "HEAD" {
                if out.head_target.is_some() {
                    return Err(RefError::MalformedLine { line: line_no });
                }
                out.head_target = Some(target.to_owned());
            }
            continue;
        }
        let oid = ObjectId::parse(left).ok_or(RefError::InvalidObjectId { line: line_no })?;
        if let Some(format) = out.refs.format()
            && format != oid.format()
        {
            return Err(RefError::MixedObjectFormat);
        }
        if let Some(base) = name.strip_suffix("^{}") {
            if previous_name.take() != Some(base) {
                return Err(RefError::OrphanPeeled { line: line_no });
            }
            // Peeled entries of ignored refs or HEAD carry no selected information.
            if let Some(tag) = previous_tag.take() {
                out.peeled.insert(tag, oid);
            }
            continue;
        }
        previous_name = Some(name);
        previous_tag = None;
        if name == "HEAD" {
            continue; // symbolic HEAD's resolved OID is not an extra ref
        }
        if !(name.starts_with("refs/heads/") || name.starts_with("refs/tags/")) {
            out.ignored += 1;
            continue;
        }
        let ref_name = RefName::parse(name).ok_or(RefError::InvalidRefName { line: line_no })?;
        if ref_name.kind() == RefKind::Tag {
            previous_tag = Some(ref_name.clone());
        }
        out.refs.insert(ref_name, oid, line_no)?;
    }
    Ok(out)
}

/// One logical ref difference. `old`/`new` `None` means absent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefChange {
    pub name: RefName,
    pub old: Option<ObjectId>,
    pub new: Option<ObjectId>,
}

/// Every created, updated or removed selected ref, by full name and full OID.
pub fn diff(old: &RefMap, new: &RefMap) -> Result<Vec<RefChange>, RefError> {
    if let (Some(a), Some(b)) = (old.format(), new.format())
        && a != b
    {
        return Err(RefError::MixedObjectFormat);
    }
    let mut changes = Vec::new();
    for (name, oid) in new.iter() {
        if old.get(name) != Some(oid) {
            changes.push(RefChange {
                name: name.clone(),
                old: old.get(name).cloned(),
                new: Some(oid.clone()),
            });
        }
    }
    for (name, oid) in old.iter() {
        if new.get(name).is_none() {
            changes.push(RefChange {
                name: name.clone(),
                old: Some(oid.clone()),
                new: None,
            });
        }
    }
    changes.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(changes)
}

#[cfg(test)]
mod tests;
