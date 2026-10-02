//! Redaction of secret material before text reaches logs, events, UI or exports.
//!
//! Scope: exact known secrets (also percent-encoded), provider-token-shaped words,
//! URL userinfo and HTTP `Authorization`/`Proxy-Authorization` header values. It is a
//! heuristic for unknown secrets. Streaming consumers must redact whole lines, so a
//! token is never split across two redaction calls. Runs in linear time.

use gbm_domain::contract::secret::SecretString;

pub const REDACTED: &str = "[REDACTED]";

/// GitHub and GitLab token prefixes (PAT, OAuth, runner, deploy, CI job, feed, ...).
const TOKEN_PREFIXES: [&str; 16] = [
    "github_pat_",
    "ghp_",
    "gho_",
    "ghu_",
    "ghs_",
    "ghr_",
    "glpat-",
    "gloas-",
    "glptt-",
    "glrt-",
    "glcbt-",
    "gldt-",
    "glft-",
    "glimt-",
    "glagent-",
    "glsoat-",
];
/// Minimum characters after a prefix before a word is treated as a token.
const MIN_TOKEN_BODY: usize = 8;

fn is_token_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
}

/// Redacts `text` line by line.
pub fn redact(text: &str, known: &[&SecretString]) -> String {
    let mut secrets: Vec<String> = Vec::new();
    for secret in known {
        let value = secret.expose();
        if !value.is_empty() {
            secrets.push(value.to_owned());
            let encoded = percent_encode(value);
            if encoded != value {
                secrets.push(encoded);
            }
        }
    }
    // Longest first, so a secret containing another is replaced whole.
    secrets.sort_by_key(|s| std::cmp::Reverse(s.len()));

    let mut out = String::with_capacity(text.len());
    for (index, line) in text.split('\n').enumerate() {
        if index > 0 {
            out.push('\n');
        }
        let mut line = line.to_owned();
        for secret in &secrets {
            if line.contains(secret.as_str()) {
                line = line.replace(secret.as_str(), REDACTED);
            }
        }
        let line = redact_authorization(&line);
        let line = redact_prefixed_tokens(&line);
        out.push_str(&redact_userinfo(&line));
    }
    out
}

fn percent_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for b in value.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// `Authorization: Basic xyz` / `Proxy-Authorization: Bearer xyz` keep only the name.
fn redact_authorization(line: &str) -> String {
    let lower = line.to_ascii_lowercase();
    for name in ["proxy-authorization:", "authorization:"] {
        if let Some(index) = lower.find(name) {
            let end = index + name.len();
            return format!("{} {REDACTED}", &line[..end]);
        }
    }
    line.to_owned()
}

/// Single pass: at each word boundary check the prefixes, then skip the whole word.
fn redact_prefixed_tokens(line: &str) -> String {
    let bytes = line.as_bytes();
    let mut out = String::with_capacity(line.len());
    let mut copied = 0;
    let mut i = 0;
    while i < bytes.len() {
        let boundary = i == 0 || !is_token_byte(bytes[i - 1]);
        if !boundary || !is_token_byte(bytes[i]) {
            i += 1;
            continue;
        }
        let word_end = bytes[i..]
            .iter()
            .position(|b| !is_token_byte(*b))
            .map_or(bytes.len(), |p| i + p);
        let word = &line[i..word_end];
        let is_token = TOKEN_PREFIXES
            .iter()
            .any(|p| word.starts_with(p) && word.len() - p.len() >= MIN_TOKEN_BODY);
        if is_token {
            out.push_str(&line[copied..i]);
            out.push_str(REDACTED);
            copied = word_end;
        }
        i = word_end;
    }
    out.push_str(&line[copied..]);
    out
}

/// `scheme://user:pass@host` -> `scheme://[REDACTED]@host`. The authority ends at the
/// last `@` before whitespace, so a password containing `/` is still covered.
fn redact_userinfo(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(index) = rest.find("://") {
        let (head, tail) = rest.split_at(index + 3);
        out.push_str(head);
        let word_end = tail.find(char::is_whitespace).unwrap_or(tail.len());
        match tail[..word_end].rfind('@') {
            Some(at) => {
                out.push_str(REDACTED);
                rest = &tail[at..];
            }
            None => rest = tail,
        }
    }
    out.push_str(rest);
    out
}
