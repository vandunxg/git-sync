//! Setup diagnostics: only facts this build actually probes; the rest is `unknown`.

use std::path::Path;
use std::process::Command;

use gbm_domain::contract::{Diagnostic, DiagnosticStatus};
use gbm_infrastructure::platform::PrivateRuntimeDir;
use gbm_infrastructure::secrets::KeyringStore;

fn item(key: &str, status: DiagnosticStatus, detail: impl Into<String>) -> Diagnostic {
    let mut detail: String = detail.into();
    detail.truncate(400);
    Diagnostic {
        key: key.to_owned(),
        status,
        detail,
    }
}

/// Runs `git <args>` with a cleared environment (only PATH) and returns its first line.
fn git_line(args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .map(str::to_owned)
}

pub fn collect(app_data: &Path, root: Option<&str>) -> Vec<Diagnostic> {
    use DiagnosticStatus as S;
    let mut items = Vec::new();
    items.push(match git_line(&["--version"]) {
        Some(version) => item("git", S::Available, version),
        None => item(
            "git",
            S::Missing,
            "Git was not found on PATH; backups cannot run.",
        ),
    });
    items.push(match git_line(&["lfs", "version"]) {
        Some(version) => item("git-lfs", S::Available, version),
        None => item(
            "git-lfs",
            S::Missing,
            "git-lfs was not found; LFS payloads cannot be backed up.",
        ),
    });
    items.push(match KeyringStore::connect() {
        Ok(_) => item("keyring", S::Available, "Secret Service is reachable."),
        Err(error) => item(
            "keyring",
            S::Missing,
            format!("{error}. Session-only storage is still available."),
        ),
    });
    items.push(item(
        "app-data",
        S::Available,
        format!("Application data in {}", app_data.display()),
    ));
    let runtime =
        std::env::var_os("XDG_RUNTIME_DIR").map(|base| PrivateRuntimeDir::under(Path::new(&base)));
    items.push(match runtime {
        Some(Ok(dir)) => item(
            "runtime-dir",
            S::Available,
            format!("Private runtime directory {}", dir.path().display()),
        ),
        Some(Err(error)) => item("runtime-dir", S::Missing, error.to_string()),
        None => item(
            "runtime-dir",
            S::Missing,
            "XDG_RUNTIME_DIR is not set; Git authentication cannot start.",
        ),
    });
    items.push(match root {
        Some(path) => item(
            "storage-root",
            S::Available,
            format!("Backup folder {path}"),
        ),
        None => item("storage-root", S::Unknown, "No backup folder chosen yet."),
    });
    items.push(item(
        "tray",
        S::Unknown,
        "System tray support is not checked by this build.",
    ));
    items
}
