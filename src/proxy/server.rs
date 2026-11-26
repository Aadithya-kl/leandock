use std::io::Write;
use std::net::SocketAddr;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use flate2::write::GzEncoder;
use flate2::Compression;
use http::{header, Request, Response, StatusCode};
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::server::conn::http1 as server_http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;

use crate::error::{LeanDockError, Result};
use crate::prerun::static_offload::OffloadedSite;
use crate::proxy::wake_manager::{IdleRouteConfig, WakeManager};

/// Starts all Wake-on-Request HTTP reverse proxy listeners + static frontend listeners + idle reaper,
/// shutting down cleanly on `SIGINT` / `SIGTERM`.
pub async fn run_proxy_server(
    wake_manager: Arc<WakeManager>,
    routes: Vec<IdleRouteConfig>,
    static_sites: Vec<OffloadedSite>,
) -> Result<()> {
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let mut handles = Vec::new();

    // Background idle reaper ticking every 15s.
    {
        let wm = wake_manager.clone();
        let mut rx = shutdown_rx.clone();
        handles.push(tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(15));
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        let _ = wm.reap_idle_once().await;
                    }
                    _ = rx.changed() => {
                        break;
                    }
                }
            }
        }));
    }

    // Spawn one HTTP/1.1 reverse proxy listener per Wake-on-Request route.
    for route in routes {
        let addr: SocketAddr = ([0, 0, 0, 0], route.listen_port).into();
        let listener = TcpListener::bind(addr).await.map_err(|e| {
            LeanDockError::Proxy(format!(
                "Failed to bind Wake-on-Request proxy port {} for `{}`: {}",
                route.listen_port, route.service_name, e
            ))
        })?;

        tracing::info!(
            service = %route.service_name,
            listen = %addr,
            target = %route.target_addr_string(),
            idle_timeout_secs = route.idle_timeout.as_secs(),
            "Wake-on-Request listener active"
        );

        let wm = wake_manager.clone();
        let service_name = route.service_name.clone();
        let mut rx = shutdown_rx.clone();

        handles.push(tokio::spawn(async move {
            loop {
                tokio::select! {
                    accept_res = listener.accept() => {
                        let Ok((stream, _peer)) = accept_res else { continue; };
                        let wm_conn = wm.clone();
                        let svc_conn = service_name.clone();
                        tokio::spawn(async move {
                            let io = TokioIo::new(stream);
                            let svc = service_fn(move |req: Request<Incoming>| {
                                let wm_req = wm_conn.clone();
                                let svc_req = svc_conn.clone();
                                async move { handle_proxy_request(req, wm_req, &svc_req).await }
                            });
                            if let Err(err) = server_http1::Builder::new().serve_connection(io, svc).await {
                                tracing::debug!(error = %err, "Proxy HTTP connection ended");
                            }
                        });
                    }
                    _ = rx.changed() => {
                        break;
                    }
                }
            }
        }));
    }

    // Spawn lightweight Rust static file servers for any `--extract-static` offloaded frontends.
    for site in static_sites {
        let addr: SocketAddr = ([0, 0, 0, 0], site.listen_port).into();
        let listener = TcpListener::bind(addr).await.map_err(|e| {
            LeanDockError::Proxy(format!(
                "Failed to bind static offload port {} for `{}`: {}",
                site.listen_port, site.service_name, e
            ))
        })?;

        tracing::info!(
            service = %site.service_name,
            listen = %addr,
            root = %site.output_dir.display(),
            spa = site.spa_fallback,
            "Static frontend offload server active"
        );

        let site_arc = Arc::new(site);
        let mut rx = shutdown_rx.clone();

        handles.push(tokio::spawn(async move {
            loop {
                tokio::select! {
                    accept_res = listener.accept() => {
                        let Ok((stream, _peer)) = accept_res else { continue; };
                        let site_conn = site_arc.clone();
                        tokio::spawn(async move {
                            let io = TokioIo::new(stream);
                            let svc = service_fn(move |req: Request<Incoming>| {
                                let site_req = site_conn.clone();
                                async move { Ok::<_, std::convert::Infallible>(serve_static_request(req, &site_req)) }
                            });
                            if let Err(err) = server_http1::Builder::new().serve_connection(io, svc).await {
                                tracing::debug!(error = %err, "Static HTTP connection ended");
                            }
                        });
                    }
                    _ = rx.changed() => {
                        break;
                    }
                }
            }
        }));
    }

    wait_for_shutdown_signal().await;
    tracing::info!("Shutdown signal received; stopping LeanDock proxy gracefully");
    let _ = shutdown_tx.send(true);

    for h in handles {
        let _ = h.await;
    }

    Ok(())
}

async fn handle_proxy_request(
    req: Request<Incoming>,
    wake_manager: Arc<WakeManager>,
    service_name: &str,
) -> std::result::Result<Response<Full<Bytes>>, std::convert::Infallible> {
    let cfg = match wake_manager.ensure_awake(service_name).await {
        Ok(c) => c,
        Err(err) => {
            tracing::warn!(service = %service_name, error = %err, "Wake failed; returning 503");
            return Ok(service_unavailable_response(&err.to_string()));
        }
    };

    match forward_http1(req, &cfg.target_host, cfg.target_port).await {
        Ok(resp) => Ok(resp),
        Err(err) => {
            tracing::warn!(service = %service_name, error = %err, "Upstream forwarding failed");
            Ok(service_unavailable_response(&format!(
                "Upstream `{}` did not respond: {}",
                service_name, err
            )))
        }
    }
}

async fn forward_http1(
    req: Request<Incoming>,
    target_host: &str,
    target_port: u16,
) -> Result<Response<Full<Bytes>>> {
    let target_addr = format!("{}:{}", target_host, target_port);
    let stream = TcpStream::connect(&target_addr).await.map_err(|e| {
        LeanDockError::Proxy(format!("Upstream connect to {} failed: {}", target_addr, e))
    })?;

    let io = TokioIo::new(stream);
    let (mut sender, conn) = hyper::client::conn::http1::handshake(io)
        .await
        .map_err(|e| LeanDockError::Proxy(format!("Upstream HTTP handshake failed: {}", e)))?;

    tokio::spawn(async move {
        if let Err(err) = conn.await {
            tracing::debug!(error = %err, "Upstream HTTP connection closed");
        }
    });

    let (mut parts, incoming_body) = req.into_parts();
    let body_bytes = incoming_body
        .collect()
        .await
        .map_err(|e| LeanDockError::Proxy(format!("Failed reading client body: {}", e)))?
        .to_bytes();

    strip_hop_by_hop_headers(&mut parts.headers);

    // Normalize absolute-form URIs to origin-form (`/path?query`) for HTTP/1.1 upstreams.
    if let Some(pq) = parts.uri.path_and_query() {
        if let Ok(normalized) = pq.as_str().parse::<http::Uri>() {
            parts.uri = normalized;
        }
    }

    // Ensure Host header is present for HTTP/1.1 compliance.
    if !parts.headers.contains_key(header::HOST) {
        if let Ok(val) = header::HeaderValue::from_str(&target_addr) {
            parts.headers.insert(header::HOST, val);
        }
    }

    let upstream_req = Request::from_parts(parts, Full::new(body_bytes));
    let upstream_res = sender
        .send_request(upstream_req)
        .await
        .map_err(|e| LeanDockError::Proxy(format!("Upstream send failed: {}", e)))?;

    let (mut res_parts, res_body) = upstream_res.into_parts();
    let res_bytes = res_body
        .collect()
        .await
        .map_err(|e| LeanDockError::Proxy(format!("Failed reading upstream body: {}", e)))?
        .to_bytes();

    strip_hop_by_hop_headers(&mut res_parts.headers);
    Ok(Response::from_parts(res_parts, Full::new(res_bytes)))
}

fn strip_hop_by_hop_headers(headers: &mut http::HeaderMap) {
    const HOP_BY_HOP: [&str; 7] = [
        "connection",
        "keep-alive",
        "proxy-authenticate",
        "proxy-authorization",
        "proxy-connection",
        "te",
        "transfer-encoding",
    ];
    for name in HOP_BY_HOP {
        headers.remove(name);
    }
}

fn service_unavailable_response(detail: &str) -> Response<Full<Bytes>> {
    Response::builder()
        .status(StatusCode::SERVICE_UNAVAILABLE)
        .header(header::RETRY_AFTER, "5")
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(Full::new(Bytes::from(format!(
            "503 Service Unavailable (LeanDock Wake-on-Request): {}\n",
            detail
        ))))
        .expect("valid static 503 response")
}

/// Serves a static file from `site.output_dir` with path traversal protection, gzip, and optional SPA fallback.
pub fn serve_static_request(req: Request<Incoming>, site: &OffloadedSite) -> Response<Full<Bytes>> {
    let raw_path = req.uri().path();
    let accept_gzip = req
        .headers()
        .get(header::ACCEPT_ENCODING)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.contains("gzip"))
        .unwrap_or(false);

    let resolved = match safe_resolve_static_path(&site.output_dir, raw_path) {
        Some(p) if p.is_file() => p,
        _ if site.spa_fallback => site.output_dir.join("index.html"),
        _ => {
            return Response::builder()
                .status(StatusCode::NOT_FOUND)
                .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
                .body(Full::new(Bytes::from_static(b"404 Not Found\n")))
                .expect("valid 404 response");
        }
    };

    let Ok(bytes) = std::fs::read(&resolved) else {
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Full::new(Bytes::from_static(b"404 Not Found\n")))
            .expect("valid 404 response");
    };

    let content_type = guess_content_type(&resolved);
    let mut builder = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, "public, max-age=3600");

    let payload = if accept_gzip && bytes.len() >= 1024 && is_compressible(content_type) {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
        if encoder.write_all(&bytes).is_ok() {
            if let Ok(compressed) = encoder.finish() {
                builder = builder.header(header::CONTENT_ENCODING, "gzip");
                Bytes::from(compressed)
            } else {
                Bytes::from(bytes)
            }
        } else {
            Bytes::from(bytes)
        }
    } else {
        Bytes::from(bytes)
    };

    builder
        .body(Full::new(payload))
        .expect("valid static file response")
}

fn safe_resolve_static_path(root: &Path, uri_path: &str) -> Option<PathBuf> {
    let trimmed = uri_path.trim_start_matches('/');
    let mut rel = PathBuf::new();
    for comp in Path::new(trimmed).components() {
        match comp {
            Component::Normal(part) => rel.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }

    let candidate = if rel.as_os_str().is_empty() {
        root.join("index.html")
    } else {
        let p = root.join(&rel);
        if p.is_dir() {
            p.join("index.html")
        } else {
            p
        }
    };
    Some(candidate)
}

fn guess_content_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase())
        .as_deref()
    {
        Some("html" | "htm") => "text/html; charset=utf-8",
        Some("js" | "mjs") => "application/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        Some("wasm") => "application/wasm",
        _ => "application/octet-stream",
    }
}

fn is_compressible(mime: &str) -> bool {
    mime.starts_with("text/")
        || mime.starts_with("application/javascript")
        || mime.starts_with("application/json")
        || mime == "image/svg+xml"
}

async fn wait_for_shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sigterm = signal(SignalKind::terminate()).ok();
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = async {
                if let Some(ref mut s) = sigterm {
                    s.recv().await;
                } else {
                    std::future::pending::<()>().await;
                }
            } => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
