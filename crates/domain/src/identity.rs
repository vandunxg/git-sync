//! Stable source identities, root-relative storage paths and destination slugs
//! (REQ-04, REQ-05, REQ-40; WORKSPACE_AND_FILESYSTEM.md, DISCOVERY_AND_PROVIDERS.md).
//!
//! Every derived name embeds the immutable provider repository ID, so a rename or
//! transfer never changes identity and two sources can never derive the same path.
//! Display names are sanitized to a small ASCII alphabet; they are hints, not keys.

use std::fmt;

/// Maximum bytes of one path component on common Linux filesystems.
const MAX_COMPONENT: usize = 255;
/// GitLab project path limit.
const MAX_PROJECT_PATH: usize = 255;
/// Fallback when a display name sanitizes to nothing.
const FALLBACK_NAME: &str = "repo";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityError {
    /// Not a canonical positive decimal provider ID.
    InvalidSourceId,
    /// A root-relative path is absolute, empty, traverses, or has unsafe characters.
    UnsafeRelativePath,
}

impl fmt::Display for IdentityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidSourceId => "invalid source repository id",
            Self::UnsafeRelativePath => "unsafe storage path",
        })
    }
}

impl std::error::Error for IdentityError {}

/// Immutable GitHub repository identity (numeric provider ID).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GitHubRepoId(u64);

impl GitHubRepoId {
    /// Accepts only canonical decimal text: no sign, no leading zero, no whitespace.
    pub fn parse(text: &str) -> Result<Self, IdentityError> {
        let canonical = !text.is_empty()
            && text.len() <= 20
            && text.bytes().all(|b| b.is_ascii_digit())
            && !text.starts_with('0');
        if !canonical {
            return Err(IdentityError::InvalidSourceId);
        }
        text.parse::<u64>()
            .map(Self)
            .map_err(|_| IdentityError::InvalidSourceId)
    }

    pub fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for GitHubRepoId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A validated path relative to a backup root: one or more components from
/// `[A-Za-z0-9._-]`, none empty, `.` or `..`, none longer than 255 bytes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RelativePath(Vec<String>);

impl RelativePath {
    /// Parses a stored `a/b/c` form. Absolute paths, `\`, empty segments and
    /// traversal are rejected rather than normalized.
    pub fn parse(text: &str) -> Result<Self, IdentityError> {
        if text.is_empty() || text.len() > 4096 {
            return Err(IdentityError::UnsafeRelativePath);
        }
        Self::from_components(text.split('/').map(str::to_owned).collect())
    }

    fn from_components(components: Vec<String>) -> Result<Self, IdentityError> {
        if components.is_empty() || !components.iter().all(|c| is_safe_component(c)) {
            return Err(IdentityError::UnsafeRelativePath);
        }
        Ok(Self(components))
    }

    pub fn components(&self) -> &[String] {
        &self.0
    }

    /// Case-folded form for collision checks on case-insensitive filesystems.
    pub fn collision_key(&self) -> String {
        self.to_string().to_ascii_lowercase()
    }

    /// True when `self` equals `other` or one contains the other (case-folded), i.e.
    /// the two cannot both be owned by different sources.
    pub fn overlaps(&self, other: &Self) -> bool {
        self.0
            .iter()
            .zip(&other.0)
            .all(|(a, b)| a.eq_ignore_ascii_case(b))
    }
}

impl fmt::Display for RelativePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0.join("/"))
    }
}

/// A component is safe when it is non-empty, not `.`/`..`, at most 255 bytes and
/// limited to `[A-Za-z0-9._-]` (so no separator, NUL, control or non-ASCII byte).
pub fn is_safe_component(component: &str) -> bool {
    !component.is_empty()
        && component.len() <= MAX_COMPONENT
        && component != "."
        && component != ".."
        && component
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// `.gbm/mirrors/gh-<id>.git` — the app-owned bare mirror.
pub fn mirror_relative_path(id: GitHubRepoId) -> RelativePath {
    RelativePath(vec![
        ".gbm".to_owned(),
        "mirrors".to_owned(),
        format!("gh-{id}.git"),
    ])
}

/// Parses a stored mirror path and requires the exact `.gbm/mirrors/gh-<id>.git`
/// shape, so a tampered row such as `.gbm` cannot be treated as a mirror.
pub fn parse_mirror_path(text: &str) -> Result<(RelativePath, GitHubRepoId), IdentityError> {
    let path = RelativePath::parse(text)?;
    let id = match path.components() {
        [app, mirrors, leaf] if app == ".gbm" && mirrors == "mirrors" => leaf
            .strip_prefix("gh-")
            .and_then(|rest| rest.strip_suffix(".git"))
            .ok_or(IdentityError::UnsafeRelativePath)
            .and_then(|digits| {
                GitHubRepoId::parse(digits).map_err(|_| IdentityError::UnsafeRelativePath)
            })?,
        _ => return Err(IdentityError::UnsafeRelativePath),
    };
    Ok((path, id))
}

/// Parses a stored workspace path and requires `workspace/<owner>/<name>-<id>` with
/// the given source ID as the leaf suffix.
pub fn parse_workspace_path(text: &str, id: GitHubRepoId) -> Result<RelativePath, IdentityError> {
    let path = RelativePath::parse(text)?;
    let suffix = format!("-{id}");
    match path.components() {
        [top, _owner, leaf]
            if top == "workspace" && leaf.len() > suffix.len() && leaf.ends_with(&suffix) =>
        {
            Ok(path)
        }
        _ => Err(IdentityError::UnsafeRelativePath),
    }
}

/// `.gbm/snapshots/gh-<id>` — parent of this source's immutable snapshots.
pub fn snapshots_relative_dir(id: GitHubRepoId) -> RelativePath {
    RelativePath(vec![
        ".gbm".to_owned(),
        "snapshots".to_owned(),
        format!("gh-{id}"),
    ])
}

/// `workspace/<owner>/<name>-<id>` — the optional working clone. The owner is
/// lower-cased (GitHub logins are case-insensitive) so a case-only owner rename
/// cannot split one owner into two sibling folders; the ID suffix keeps the leaf
/// unique even when names collide after sanitizing.
pub fn workspace_relative_path(owner: &str, name: &str, id: GitHubRepoId) -> RelativePath {
    let owner_dir = filesystem_name(&owner.to_ascii_lowercase(), "");
    let leaf = filesystem_name(name, &format!("-{id}"));
    RelativePath(vec!["workspace".to_owned(), owner_dir, leaf])
}

/// Sanitizes `display` into one safe component ending with `suffix`, truncating the
/// display part so the whole fits in 255 bytes. Hidden names (leading `.`) and
/// Windows device names get a `_` prefix so the folder is visible and portable.
fn filesystem_name(display: &str, suffix: &str) -> String {
    let mut stem: String = display
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '-'
            }
        })
        .collect();
    if stem.is_empty() || stem.bytes().all(|b| b == b'.') {
        stem = FALLBACK_NAME.to_owned();
    }
    if stem.starts_with('.') || is_device_name(&stem) {
        stem.insert(0, '_');
    }
    stem.truncate(MAX_COMPONENT.saturating_sub(suffix.len()));
    stem.push_str(suffix);
    stem
}

fn is_device_name(stem: &str) -> bool {
    let base = stem
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    matches!(base.as_str(), "con" | "prn" | "aux" | "nul")
        || ((base.starts_with("com") || base.starts_with("lpt"))
            && base.len() == 4
            && base.as_bytes().get(3).is_some_and(u8::is_ascii_digit))
}

/// Proposed GitLab project path `<source-name>-gh<id>` (DISCOVERY_AND_PROVIDERS.md).
/// Lower-case alphanumeric runs joined by single `-` (any other character, including
/// `.` and `_`, separates runs), so it starts and ends alphanumeric, never ends in
/// `.git`/`.atom` and has no consecutive specials; at most 255 bytes, suffix intact.
pub fn proposed_project_path(source_name: &str, id: GitHubRepoId) -> String {
    let suffix = format!("-gh{id}");
    let mut stem = String::new();
    let mut pending_separator = false;
    for c in source_name.chars() {
        let lower = c.to_ascii_lowercase();
        if lower.is_ascii_lowercase() || lower.is_ascii_digit() {
            if pending_separator && !stem.is_empty() {
                stem.push('-');
            }
            pending_separator = false;
            stem.push(lower);
        } else {
            pending_separator = true;
        }
    }
    if stem.is_empty() {
        stem = FALLBACK_NAME.to_owned();
    }
    stem.truncate(MAX_PROJECT_PATH.saturating_sub(suffix.len()));
    while stem.ends_with('-') {
        stem.pop();
    }
    stem.push_str(&suffix);
    stem
}

/// Case-folded project path for comparisons (GitLab paths are case-insensitive).
pub fn project_path_key(path: &str) -> String {
    path.to_ascii_lowercase()
}

#[cfg(test)]
mod tests;
