//! Tauri bootstrap: navigation guard plus the typed app-owned command facade.

mod commands;

/// Vite dev server origin from `tauri.conf.json` `build.devUrl` (debug builds only).
const DEV_SERVER_PORT: u16 = 3000;

/// Allows only navigation to the bundled app origin (and the Vite dev server in debug
/// builds). Everything else is refused without opening it: safe external-open actions
/// are app-owned commands added by later tasks, not a renderer-wide opener.
fn is_internal_navigation(url: &tauri::Url) -> bool {
    match url.scheme() {
        "tauri" => true,
        "http" | "https" => match url.host_str() {
            Some("tauri.localhost") => true,
            Some("localhost") => {
                cfg!(debug_assertions) && url.port_or_known_default() == Some(DEV_SERVER_PORT)
            }
            _ => false,
        },
        _ => false,
    }
}

fn navigation_guard<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::<R>::new("navigation-guard")
        .on_navigation(|_webview, url| is_internal_navigation(url))
        .build()
}

/// Starts the desktop shell and blocks until the application exits.
pub fn run() -> tauri::Result<()> {
    tauri::Builder::default()
        .plugin(navigation_guard())
        .manage(commands::Facade::new(
            gbm_application::ipc::NotYetImplemented,
        ))
        .invoke_handler(commands::invoke_handler())
        .run(tauri::generate_context!())
}

#[cfg(test)]
mod tests {
    use super::is_internal_navigation;

    fn allowed(url: &str) -> bool {
        match tauri::Url::parse(url) {
            Ok(parsed) => is_internal_navigation(&parsed),
            Err(_) => false,
        }
    }

    #[test]
    fn bundled_origins_are_internal() {
        assert!(allowed("tauri://localhost/index.html"));
        assert!(allowed("http://tauri.localhost/"));
    }

    #[test]
    fn external_and_lookalike_origins_are_refused() {
        assert!(!allowed("https://github.com/"));
        assert!(!allowed("https://tauri.localhost.example.com/"));
        assert!(!allowed("http://127.0.0.1:3000/"));
        assert!(!allowed("file:///etc/passwd"));
        assert!(!allowed("mailto:someone@example.invalid"));
    }

    #[test]
    fn dev_server_is_internal_only_in_debug_builds() {
        assert_eq!(allowed("http://localhost:3000/"), cfg!(debug_assertions));
        assert!(!allowed("http://localhost:8080/"));
        assert!(!allowed("http://localhost/"));
    }
}
