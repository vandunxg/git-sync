//! Test-only loopback HTTP server ("controlled adapter" for TEST-PAT). It serves canned
//! responses in order and records each raw request head for assertions. Compiled only
//! under `cfg(test)`; it is never a fallback for a real provider connection.

use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[derive(Clone)]
pub(crate) enum Reply {
    /// Status line code, extra headers, body.
    Respond(u16, Vec<(String, String)>, String),
    /// Accept the request and never answer (timeout tests).
    Hang,
}

pub(crate) struct TestServer {
    pub origin: String,
    requests: Arc<Mutex<Vec<String>>>,
}

impl TestServer {
    pub(crate) async fn start(replies: Vec<Reply>) -> std::io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let origin = format!("http://{}/", listener.local_addr()?);
        // `{{ORIGIN}}/` (and `{{HOSTPORT}}/`) in a body are replaced by this server's
        // origin (and its host:port/), for clone URLs.
        let replies: Vec<Reply> = replies
            .into_iter()
            .map(|reply| match reply {
                Reply::Respond(code, headers, body) => {
                    let host = origin.trim_start_matches("http://");
                    let body = body
                        .replace("{{ORIGIN}}/", &origin)
                        .replace("{{HOSTPORT}}/", host);
                    Reply::Respond(code, headers, body)
                }
                Reply::Hang => Reply::Hang,
            })
            .collect();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&requests);
        tokio::spawn(async move {
            for reply in replies {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                let mut head = Vec::new();
                let mut buf = [0u8; 4096];
                while !head.windows(4).any(|w| w == b"\r\n\r\n") {
                    match socket.read(&mut buf).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => head.extend_from_slice(&buf[..n]),
                    }
                }
                // Read a request body announced by content-length (POST tests).
                let text = String::from_utf8_lossy(&head).to_ascii_lowercase();
                let body_start = text.find("\r\n\r\n").map_or(head.len(), |i| i + 4);
                let length = text
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length:"))
                    .and_then(|v| v.trim().parse::<usize>().ok())
                    .unwrap_or(0);
                while head.len() < body_start + length {
                    match socket.read(&mut buf).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => head.extend_from_slice(&buf[..n]),
                    }
                }
                if let Ok(mut log) = seen.lock() {
                    log.push(String::from_utf8_lossy(&head).into_owned());
                }
                match reply {
                    Reply::Hang => {
                        // Hold the connection open without blocking later replies.
                        tokio::spawn(async move {
                            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                            drop(socket);
                        });
                    }
                    Reply::Respond(code, headers, body) => {
                        let mut text = format!(
                            "HTTP/1.1 {code} X\r\ncontent-length: {}\r\nconnection: close\r\n",
                            body.len()
                        );
                        for (name, value) in headers {
                            text.push_str(&format!("{name}: {value}\r\n"));
                        }
                        text.push_str("\r\n");
                        text.push_str(&body);
                        let _ignored = socket.write_all(text.as_bytes()).await;
                    }
                }
            }
        });
        Ok(Self { origin, requests })
    }

    pub(crate) fn requests(&self) -> Vec<String> {
        self.requests.lock().map(|r| r.clone()).unwrap_or_default()
    }
}
