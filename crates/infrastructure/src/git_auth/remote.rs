//! Validation of HTTPS remotes against the V1 provider allowlist (R-SEC execution rules).

use std::fmt;

/// V1 production Git hosts. Enterprise/custom hosts need a separate extension review.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AllowedHost {
    GitHub,
    GitLab,
}

impl AllowedHost {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::GitHub => "github.com",
            Self::GitLab => "gitlab.com",
        }
    }

    fn parse(host: &str) -> Option<Self> {
        match host {
            "github.com" => Some(Self::GitHub),
            "gitlab.com" => Some(Self::GitLab),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteError {
    NotHttps,
    /// `user[:password]@` in the URL: credentials must never travel in remotes.
    HasUserinfo,
    HostNotAllowed,
    /// Explicit port, query, fragment, encoded or unsafe path characters, dot segments.
    UnsafePath,
}

impl fmt::Display for RemoteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NotHttps => "only https remotes are allowed",
            Self::HasUserinfo => "remote URLs must not contain credentials",
            Self::HostNotAllowed => "the remote host is not an allowed provider",
            Self::UnsafePath => "the remote path is not a plain repository path",
        })
    }
}

impl std::error::Error for RemoteError {}

/// An allowlisted HTTPS repository location, as Git reports it to credential helpers
/// with `credential.useHttpPath=true` (`host` + `path` without the leading slash).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RemoteTarget {
    host: AllowedHost,
    path: String,
}

impl RemoteTarget {
    /// Parses `https://<allowed-host>/<segment>/.../<repo>[.git]`.
    pub fn from_url(url: &str) -> Result<Self, RemoteError> {
        let rest = url.strip_prefix("https://").ok_or(RemoteError::NotHttps)?;
        let (authority, path) = rest.split_once('/').ok_or(RemoteError::UnsafePath)?;
        if authority.contains('@') {
            return Err(RemoteError::HasUserinfo);
        }
        let host = AllowedHost::parse(authority).ok_or(RemoteError::HostNotAllowed)?;
        Self::new(host, path)
    }

    pub fn new(host: AllowedHost, path: &str) -> Result<Self, RemoteError> {
        let segments: Vec<&str> = path.split('/').collect();
        let valid = segments.len() >= 2
            && segments.iter().all(|segment| {
                !segment.is_empty()
                    && *segment != "."
                    && *segment != ".."
                    && !segment.starts_with(['-', '.'])
                    && segment
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
            });
        if !valid {
            return Err(RemoteError::UnsafePath);
        }
        Ok(Self {
            host,
            path: path.to_owned(),
        })
    }

    pub fn host(&self) -> AllowedHost {
        self.host
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    /// Credential-free URL for Git argv/config.
    pub fn url(&self) -> String {
        format!("https://{}/{}", self.host.as_str(), self.path)
    }
}
