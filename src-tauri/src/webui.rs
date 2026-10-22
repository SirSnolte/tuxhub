//! Integration level 1: embed the web interface of a tool.
//!
//! Many web interfaces forbid embedding through `X-Frame-Options` or
//! `Content-Security-Policy: frame-ancestors`. tuxhub therefore serves local
//! web interfaces through `toolweb://<id>/<path>` and removes these headers.
//! Only loopback addresses are proxied, so tuxhub is never an open proxy.
//! WebSockets are not proxied; the separate window covers that case.

use crate::{Hub, tools};
use reqwest::Url;
use std::sync::OnceLock;
use std::time::Duration;
use tauri::http::{HeaderName, Request, Response, StatusCode, header};
use tauri::{AppHandle, Manager, Runtime, WebviewUrl, WebviewWindowBuilder};

fn client() -> &'static reqwest::Client {
    static C: OnceLock<reqwest::Client> = OnceLock::new();
    C.get_or_init(|| {
        reqwest::Client::builder()
            // Redirects are passed through to the embedded page.
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(30))
            .build()
            .expect("HTTP client")
    })
}

pub fn is_loopback(url: &Url) -> bool {
    matches!(url.scheme(), "http" | "https")
        && match url.host() {
            Some(url::Host::Domain(d)) => d == "localhost",
            Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
            Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
            None => false,
        }
}

/// Web interface URL of an installed tool.
pub fn web_url<R: Runtime>(app: &AppHandle<R>, id: &str) -> Result<Url, String> {
    let hub = app.state::<Hub>();
    let inst = tools::load(&hub.tools_dir, id)?;
    let m = inst.manifest(&hub.tools_dir);
    let raw = m.web_url.ok_or("this tool has no web interface")?;
    Url::parse(&raw).map_err(|e| format!("invalid web_url: {e}"))
}

/// Whether the web interface responds.
pub async fn reachable(url: &Url) -> bool {
    client()
        .get(url.clone())
        .timeout(Duration::from_millis(1500))
        .send()
        .await
        .is_ok()
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn error_page(status: StatusCode, text: &str) -> Response<Vec<u8>> {
    let body = format!(
        "<!doctype html><meta charset=utf-8><body style=\"font:14px system-ui;padding:20px;color:#888\">{}</body>",
        html_escape(text)
    );
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .body(body.into_bytes())
        .unwrap()
}

/// Forwards a request for `toolweb://<id>/…` to the web interface of the tool.
pub async fn proxy<R: Runtime>(app: &AppHandle<R>, req: Request<Vec<u8>>) -> Response<Vec<u8>> {
    let id = req.uri().host().unwrap_or_default().to_string();
    let base = match web_url(app, &id) {
        Ok(u) if is_loopback(&u) => u,
        Ok(_) => {
            return error_page(
                StatusCode::FORBIDDEN,
                "Only local web interfaces can be embedded.",
            );
        }
        Err(e) => return error_page(StatusCode::NOT_FOUND, &e),
    };
    let origin = base.origin().ascii_serialization();
    let path = req
        .uri()
        .path_and_query()
        .map(|p| p.as_str())
        .unwrap_or("/");
    let Ok(target) = Url::parse(&format!("{origin}{path}")) else {
        return error_page(StatusCode::BAD_REQUEST, "Invalid path");
    };

    let method = reqwest::Method::from_bytes(req.method().as_str().as_bytes())
        .unwrap_or(reqwest::Method::GET);
    let mut out = client().request(method, target);
    for (name, value) in req.headers() {
        // Host, Origin and Referer would expose `toolweb://` and trip checks in the tool.
        if !matches!(
            name.as_str(),
            "host" | "origin" | "referer" | "connection" | "content-length"
        ) {
            out = out.header(name.as_str(), value.as_bytes());
        }
    }
    let resp = match out.body(req.into_body()).send().await {
        Ok(r) => r,
        Err(e) => {
            return error_page(
                StatusCode::BAD_GATEWAY,
                &format!("The web interface is not reachable: {e}"),
            );
        }
    };

    let mut builder = Response::builder().status(resp.status().as_u16());
    for (name, value) in resp.headers() {
        let n = name.as_str();
        if matches!(
            n,
            "x-frame-options"
                | "content-security-policy"
                | "content-length"
                | "transfer-encoding"
                | "connection"
        ) {
            continue;
        }
        // Route absolute redirects to the tool itself back through the proxy.
        if n == "location"
            && let Ok(loc) = value.to_str()
            && let Some(rest) = loc.strip_prefix(&origin)
        {
            builder = builder.header(header::LOCATION, format!("toolweb://{id}{rest}"));
            continue;
        }
        if let Ok(hn) = HeaderName::from_bytes(n.as_bytes()) {
            builder = builder.header(hn, value.as_bytes());
        }
    }
    match resp.bytes().await {
        Ok(body) => builder
            .body(body.to_vec())
            .unwrap_or_else(|_| error_page(StatusCode::BAD_GATEWAY, "Unreadable response")),
        Err(e) => error_page(StatusCode::BAD_GATEWAY, &e.to_string()),
    }
}

/// Opens the web interface in a separate window (supports WebSockets).
pub fn open_window(app: &AppHandle, id: &str, title: &str) -> Result<(), String> {
    let url = web_url(app, id)?;
    let label = format!("web-{}", id.replace('.', "-"));
    if let Some(w) = app.get_webview_window(&label) {
        let _ = w.show();
        let _ = w.set_focus();
        return Ok(());
    }
    WebviewWindowBuilder::new(app, label, WebviewUrl::External(url))
        .title(title)
        .inner_size(1100.0, 760.0)
        .build()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::path::PathBuf;

    /// Minimal server: one response per connection, records the requests.
    fn serve(
        responses: impl FnOnce(u16) -> Vec<String>,
    ) -> (u16, std::sync::mpsc::Receiver<String>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let responses = responses(port);
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for resp in responses {
                let (mut sock, _) = listener.accept().unwrap();
                let mut buf = [0u8; 4096];
                let n = sock.read(&mut buf).unwrap();
                tx.send(String::from_utf8_lossy(&buf[..n]).into_owned())
                    .unwrap();
                sock.write_all(resp.as_bytes()).unwrap();
            }
        });
        (port, rx)
    }

    fn mock_hub(web_url: &str) -> (tauri::App<tauri::test::MockRuntime>, PathBuf) {
        let app = tauri::test::mock_app();
        let base = std::env::temp_dir().join(format!(
            "tuxhub-web-{}-{}",
            std::process::id(),
            web_url.len()
        ));
        let tool_dir = base.join("tool");
        let tools_dir = base.join("tools");
        std::fs::create_dir_all(&tool_dir).unwrap();
        std::fs::create_dir_all(tools_dir.join("w")).unwrap();
        std::fs::write(
            tool_dir.join("tuxhub.toml"),
            format!("id = \"w\"\nname = \"W\"\nweb_url = \"{web_url}\"\n"),
        )
        .unwrap();
        std::fs::write(
            tools_dir.join("w/tool.json"),
            format!(
                r#"{{"entry":{{"id":"w","name":"W","source":{{"type":"local","path":"{}"}}}}}}"#,
                tool_dir.display()
            ),
        )
        .unwrap();
        app.manage(Hub::for_test(tools_dir, base.join("settings")));
        (app, base)
    }

    #[tokio::test]
    async fn proxy_strips_frame_blockers_and_rewrites_redirects() {
        let (port, rx) = serve(|port| {
            vec![
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nX-Frame-Options: DENY\r\n\
                 Content-Security-Policy: frame-ancestors 'none'\r\nX-Keep: yes\r\nContent-Length: 5\r\n\r\nhallo"
                    .to_string(),
                format!("HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:{port}/login?a=b\r\nContent-Length: 0\r\n\r\n"),
                "HTTP/1.1 302 Found\r\nLocation: https://example.com/\r\nContent-Length: 0\r\n\r\n".to_string(),
            ]
        });
        let (app, base) = mock_hub(&format!("http://127.0.0.1:{port}/ui/"));

        let req = Request::builder()
            .uri("toolweb://w/ui/index.html?x=1")
            .header("origin", "toolweb://w")
            .body(vec![])
            .unwrap();
        let resp = proxy(app.handle(), req).await;
        let seen = rx.recv().unwrap();
        assert!(seen.starts_with("GET /ui/index.html?x=1 "), "{seen}");
        assert!(
            !seen.to_lowercase().contains("origin:"),
            "the Origin header must not be forwarded"
        );
        assert_eq!(resp.status(), 200);
        assert_eq!(resp.body(), b"hallo");
        assert!(resp.headers().get("x-frame-options").is_none());
        assert!(resp.headers().get("content-security-policy").is_none());
        assert_eq!(resp.headers().get("x-keep").unwrap(), "yes");

        // A redirect to the tool itself goes back through the proxy.
        let req = Request::builder().uri("toolweb://w/").body(vec![]).unwrap();
        let resp = proxy(app.handle(), req).await;
        assert_eq!(resp.status(), 302);
        assert_eq!(
            resp.headers().get("location").unwrap(),
            "toolweb://w/login?a=b"
        );

        // Redirects to other hosts are left unchanged.
        let req = Request::builder()
            .uri("toolweb://w/x")
            .body(vec![])
            .unwrap();
        let resp = proxy(app.handle(), req).await;
        assert_eq!(
            resp.headers().get("location").unwrap(),
            "https://example.com/"
        );
        std::fs::remove_dir_all(base).unwrap();
    }

    #[tokio::test]
    async fn proxy_refuses_non_loopback() {
        let (app, base) = mock_hub("http://192.168.1.10:8080/");
        let req = Request::builder().uri("toolweb://w/").body(vec![]).unwrap();
        assert_eq!(proxy(app.handle(), req).await.status(), 403);
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn loopback_only() {
        let ok = [
            "http://127.0.0.1:8080/",
            "http://localhost:3000/x",
            "http://[::1]:9000/",
        ];
        let bad = [
            "http://192.168.1.5/",
            "https://example.com/",
            "file:///etc/passwd",
            "http://127.0.0.1.evil.com/",
        ];
        for u in ok {
            assert!(is_loopback(&Url::parse(u).unwrap()), "{u}");
        }
        for u in bad {
            assert!(!is_loopback(&Url::parse(u).unwrap()), "{u}");
        }
    }
}
