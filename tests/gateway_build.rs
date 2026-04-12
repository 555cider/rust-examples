use http_body_util::{BodyExt, Empty, Full};
use hyper::{body::Bytes, Request, Response, StatusCode};
use hyper_gateway::{config, config::Protocol, entity, logger, route, throttle, trace};
use hyper_util::rt::{TokioExecutor, TokioIo};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName};
use std::{
    collections::BTreeMap,
    convert::TryFrom,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::task::JoinHandle;

fn temp_db_path() -> PathBuf {
    std::env::temp_dir().join(format!("gateway-throttle-{}.db", fastrand::u64(..)))
}

fn assert_temp_db_artifacts_removed(db_path: &Path) {
    for path in [
        db_path.to_path_buf(),
        PathBuf::from(format!("{}-journal", db_path.display())),
        PathBuf::from(format!("{}-shm", db_path.display())),
        PathBuf::from(format!("{}-wal", db_path.display())),
    ] {
        assert!(
            !path.exists(),
            "temporary throttle database artifact should be removed: {}",
            path.display()
        );
    }
}

fn wait_for_temp_db_artifacts_removed(db_path: &Path) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
    loop {
        if [
            db_path.to_path_buf(),
            PathBuf::from(format!("{}-journal", db_path.display())),
            PathBuf::from(format!("{}-shm", db_path.display())),
            PathBuf::from(format!("{}-wal", db_path.display())),
        ]
        .iter()
        .all(|path| !path.exists())
        {
            return;
        }

        if std::time::Instant::now() >= deadline {
            assert_temp_db_artifacts_removed(db_path);
        }

        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

fn cleanup_temp_db_artifacts(db_path: &Path) -> std::io::Result<()> {
    for path in [
        db_path.to_path_buf(),
        PathBuf::from(format!("{}-journal", db_path.display())),
        PathBuf::from(format!("{}-shm", db_path.display())),
        PathBuf::from(format!("{}-wal", db_path.display())),
    ] {
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(err),
        }
    }

    Ok(())
}

fn cleanup_temp_db_artifacts_with_retry(db_path: &Path) -> std::io::Result<()> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);

    loop {
        match cleanup_temp_db_artifacts(db_path) {
            Ok(()) => return Ok(()),
            Err(err) if std::time::Instant::now() >= deadline => return Err(err),
            Err(_) => std::thread::sleep(std::time::Duration::from_millis(10)),
        }
    }
}

struct TempDbGuard {
    db_path: PathBuf,
}

impl TempDbGuard {
    fn new(db_path: PathBuf) -> Self {
        Self { db_path }
    }

    fn path(&self) -> &PathBuf {
        &self.db_path
    }

    fn cleanup_with_retry(&self) -> std::io::Result<()> {
        cleanup_temp_db_artifacts_with_retry(&self.db_path)
    }
}

struct SpawnedGateway {
    addr: std::net::SocketAddr,
    task: Option<JoinHandle<()>>,
    db_guard: TempDbGuard,
}

impl SpawnedGateway {
    async fn finish(mut self) {
        self.task.take().unwrap().await.unwrap();
        let db_path = self.db_guard.path().clone();
        drop(self);
        assert_temp_db_artifacts_removed(&db_path);
    }
}

struct SpawnedUpstream {
    addr: std::net::SocketAddr,
    task: Option<JoinHandle<()>>,
}

struct TlsFixture {
    cert_path: PathBuf,
    key_path: PathBuf,
    cert_der: Vec<u8>,
    key_der: Vec<u8>,
}

impl SpawnedUpstream {
    async fn finish(mut self) {
        self.task.take().unwrap().await.unwrap();
    }
}

impl Drop for TempDbGuard {
    fn drop(&mut self) {
        let _ = self.cleanup_with_retry();
    }
}

impl Drop for SpawnedGateway {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }

        let _ = self.db_guard.cleanup_with_retry();
    }
}

impl Drop for SpawnedUpstream {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

impl Drop for TlsFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.cert_path);
        let _ = std::fs::remove_file(&self.key_path);
    }
}

#[test]
fn gateway_library_exports_public_modules() {
    assert_eq!(format!("{:?}", Protocol::Http1), "Http1");
    let _ = entity::get_gateway_response;
    let _ = logger::init_logger as fn();
    let _ = route::run;
    let _ = throttle::init_throttle;
    let _ = trace::new;

    let db_guard = TempDbGuard::new(temp_db_path());
    let db_path = db_guard.path().clone();
    let pool =
        throttle::init_throttle_at(&db_path).expect("test-friendly throttle API should work");
    drop(pool);
    drop(db_guard);
    assert_temp_db_artifacts_removed(&db_path);
}

async fn spawn_gateway_once(route_config: config::Route, db_path: PathBuf) -> SpawnedGateway {
    spawn_gateway_with_routes(Arc::from([route_config]), db_path).await
}

async fn spawn_gateway_with_routes(
    routes: Arc<[config::Route]>,
    db_path: PathBuf,
) -> SpawnedGateway {
    let throttle_pool = throttle::init_throttle_at(&db_path).expect("throttle pool");
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();

    let task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        route::run(stream, routes, throttle_pool).await.unwrap();
    });

    SpawnedGateway {
        addr,
        task: Some(task),
        db_guard: TempDbGuard::new(db_path),
    }
}

async fn spawn_http1_upstream() -> SpawnedUpstream {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();

    let task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let service =
            hyper::service::service_fn(|request: Request<hyper::body::Incoming>| async move {
                let status = if request.headers().contains_key("traceparent") {
                    StatusCode::OK
                } else {
                    StatusCode::INTERNAL_SERVER_ERROR
                };

                Ok::<_, hyper::Error>(
                    Response::builder()
                        .status(status)
                        .body(Full::new(Bytes::from_static(b"http1")))
                        .unwrap(),
                )
            });

        hyper::server::conn::http1::Builder::new()
            .serve_connection(TokioIo::new(stream), service)
            .await
            .ok();
    });

    SpawnedUpstream {
        addr,
        task: Some(task),
    }
}

async fn spawn_http1_upstream_with_body_for_connections(
    body: &'static [u8],
    connection_count: usize,
) -> SpawnedUpstream {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();

    let task = tokio::spawn(async move {
        for _ in 0..connection_count {
            let (stream, _) = listener.accept().await.unwrap();
            let service = hyper::service::service_fn(
                move |_request: Request<hyper::body::Incoming>| async move {
                    Ok::<_, hyper::Error>(
                        Response::builder()
                            .status(StatusCode::OK)
                            .body(Full::new(Bytes::from_static(body)))
                            .unwrap(),
                    )
                },
            );

            hyper::server::conn::http1::Builder::new()
                .serve_connection(TokioIo::new(stream), service)
                .await
                .ok();
        }
    });

    SpawnedUpstream {
        addr,
        task: Some(task),
    }
}

async fn spawn_http1_upstream_echo_path() -> SpawnedUpstream {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();

    let task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let service =
            hyper::service::service_fn(|request: Request<hyper::body::Incoming>| async move {
                let path = request
                    .uri()
                    .path_and_query()
                    .map(|value| value.as_str().to_string())
                    .unwrap_or_else(|| "/".to_string());

                Ok::<_, hyper::Error>(
                    Response::builder()
                        .status(StatusCode::OK)
                        .body(Full::new(Bytes::from(path.into_bytes())))
                        .unwrap(),
                )
            });

        hyper::server::conn::http1::Builder::new()
            .serve_connection(TokioIo::new(stream), service)
            .await
            .ok();
    });

    SpawnedUpstream {
        addr,
        task: Some(task),
    }
}

async fn spawn_http1_upstream_echo_filter_headers() -> SpawnedUpstream {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();

    let task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let service =
            hyper::service::service_fn(|request: Request<hyper::body::Incoming>| async move {
                let added = request
                    .headers()
                    .get("x-added-by-gateway")
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or("missing");
                let removed = request.headers().contains_key("x-remove-me");
                let body = format!("x-added-by-gateway={added};x-remove-me={removed}");

                Ok::<_, hyper::Error>(
                    Response::builder()
                        .status(StatusCode::OK)
                        .body(Full::new(Bytes::from(body.into_bytes())))
                        .unwrap(),
                )
            });

        hyper::server::conn::http1::Builder::new()
            .serve_connection(TokioIo::new(stream), service)
            .await
            .ok();
    });

    SpawnedUpstream {
        addr,
        task: Some(task),
    }
}

async fn spawn_http1_upstream_with_response_headers() -> SpawnedUpstream {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();

    let task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let service =
            hyper::service::service_fn(|_request: Request<hyper::body::Incoming>| async move {
                Ok::<_, hyper::Error>(
                    Response::builder()
                        .status(StatusCode::OK)
                        .header("x-upstream-remove", "yes")
                        .header("x-upstream-keep", "ok")
                        .body(Full::new(Bytes::from_static(b"response-filter")))
                        .unwrap(),
                )
            });

        hyper::server::conn::http1::Builder::new()
            .serve_connection(TokioIo::new(stream), service)
            .await
            .ok();
    });

    SpawnedUpstream {
        addr,
        task: Some(task),
    }
}

async fn spawn_slow_http1_upstream(delay_ms: u64) -> SpawnedUpstream {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();

    let task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let service = hyper::service::service_fn(
            move |_request: Request<hyper::body::Incoming>| async move {
                tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                Ok::<_, hyper::Error>(
                    Response::builder()
                        .status(StatusCode::OK)
                        .body(Full::new(Bytes::from_static(b"slow-http1")))
                        .unwrap(),
                )
            },
        );

        hyper::server::conn::http1::Builder::new()
            .serve_connection(TokioIo::new(stream), service)
            .await
            .ok();
    });

    SpawnedUpstream {
        addr,
        task: Some(task),
    }
}

async fn spawn_http1_upstream_fail_once_then_succeed() -> SpawnedUpstream {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();

    let task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        drop(stream);

        let (stream, _) = listener.accept().await.unwrap();
        let service = hyper::service::service_fn(
            move |_request: Request<hyper::body::Incoming>| async move {
                Ok::<_, hyper::Error>(
                    Response::builder()
                        .status(StatusCode::OK)
                        .body(Full::new(Bytes::from_static(b"breaker-recovered")))
                        .unwrap(),
                )
            },
        );

        hyper::server::conn::http1::Builder::new()
            .serve_connection(TokioIo::new(stream), service)
            .await
            .ok();
    });

    SpawnedUpstream {
        addr,
        task: Some(task),
    }
}

async fn spawn_h2c_upstream() -> SpawnedUpstream {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();

    let task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let service =
            hyper::service::service_fn(|request: Request<hyper::body::Incoming>| async move {
                let status = if request.headers().contains_key("traceparent") {
                    StatusCode::OK
                } else {
                    StatusCode::INTERNAL_SERVER_ERROR
                };

                Ok::<_, hyper::Error>(
                    Response::builder()
                        .status(status)
                        .body(Full::new(Bytes::from_static(b"h2c")))
                        .unwrap(),
                )
            });

        hyper::server::conn::http2::Builder::new(TokioExecutor::new())
            .serve_connection(TokioIo::new(stream), service)
            .await
            .unwrap();
    });

    SpawnedUpstream {
        addr,
        task: Some(task),
    }
}

fn create_localhost_tls_fixture() -> TlsFixture {
    let certified = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])
        .expect("localhost certificate");
    let cert_der = certified.cert.der().to_vec();
    let key_der = certified.key_pair.serialize_der();
    let cert_path =
        std::env::temp_dir().join(format!("gateway-test-cert-{}.der", fastrand::u64(..)));
    let key_path = std::env::temp_dir().join(format!("gateway-test-key-{}.der", fastrand::u64(..)));
    std::fs::write(&cert_path, &cert_der).expect("write test cert");
    std::fs::write(&key_path, &key_der).expect("write test key");

    TlsFixture {
        cert_path,
        key_path,
        cert_der,
        key_der,
    }
}

async fn spawn_https_http1_upstream(fixture: &TlsFixture) -> SpawnedUpstream {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    let certs = vec![CertificateDer::from(fixture.cert_der.clone())];
    let key = PrivateKeyDer::from(PrivatePkcs8KeyDer::from(fixture.key_der.clone()));
    let tls_config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .expect("server config");
    let tls_acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(tls_config));

    let task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let stream = tls_acceptor.accept(stream).await.unwrap();
        let service =
            hyper::service::service_fn(|request: Request<hyper::body::Incoming>| async move {
                let status = if request.headers().contains_key("traceparent") {
                    StatusCode::OK
                } else {
                    StatusCode::INTERNAL_SERVER_ERROR
                };

                Ok::<_, hyper::Error>(
                    Response::builder()
                        .status(status)
                        .body(Full::new(Bytes::from_static(b"https-http1")))
                        .unwrap(),
                )
            });

        hyper::server::conn::http1::Builder::new()
            .serve_connection(TokioIo::new(stream), service)
            .await
            .unwrap();
    });

    SpawnedUpstream {
        addr,
        task: Some(task),
    }
}

async fn spawn_https_h2_upstream(fixture: &TlsFixture) -> SpawnedUpstream {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    let certs = vec![CertificateDer::from(fixture.cert_der.clone())];
    let key = PrivateKeyDer::from(PrivatePkcs8KeyDer::from(fixture.key_der.clone()));
    let mut tls_config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .expect("server config");
    tls_config.alpn_protocols = vec![b"h2".to_vec()];
    let tls_acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(tls_config));

    let task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let stream = tls_acceptor.accept(stream).await.unwrap();
        let service =
            hyper::service::service_fn(|request: Request<hyper::body::Incoming>| async move {
                let status = if request.headers().contains_key("traceparent") {
                    StatusCode::OK
                } else {
                    StatusCode::INTERNAL_SERVER_ERROR
                };

                Ok::<_, hyper::Error>(
                    Response::builder()
                        .status(status)
                        .body(Full::new(Bytes::from_static(b"https-h2")))
                        .unwrap(),
                )
            });

        hyper::server::conn::http2::Builder::new(TokioExecutor::new())
            .serve_connection(TokioIo::new(stream), service)
            .await
            .unwrap();
    });

    SpawnedUpstream {
        addr,
        task: Some(task),
    }
}

async fn send_http1_request(addr: std::net::SocketAddr) -> (StatusCode, Bytes) {
    send_http1_request_with(addr, hyper::Method::GET, "gateway.local", "/").await
}

async fn send_http1_request_with(
    addr: std::net::SocketAddr,
    method: hyper::Method,
    host: &str,
    path: &str,
) -> (StatusCode, Bytes) {
    send_http1_request_with_headers(addr, method, host, path, &[]).await
}

async fn send_http1_request_with_headers(
    addr: std::net::SocketAddr,
    method: hyper::Method,
    host: &str,
    path: &str,
    extra_headers: &[(&str, &str)],
) -> (StatusCode, Bytes) {
    let (status, _, body) = send_http1_request_full(addr, method, host, path, extra_headers).await;
    (status, body)
}

async fn send_http1_request_full(
    addr: std::net::SocketAddr,
    method: hyper::Method,
    host: &str,
    path: &str,
    extra_headers: &[(&str, &str)],
) -> (StatusCode, hyper::HeaderMap, Bytes) {
    let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let (mut sender, conn) = hyper::client::conn::http1::Builder::new()
        .handshake::<_, Empty<Bytes>>(TokioIo::new(stream))
        .await
        .unwrap();

    let conn_task = tokio::spawn(async move {
        conn.await.unwrap();
    });

    let mut request_builder = Request::builder()
        .uri(format!("http://{host}{path}"))
        .method(method)
        .header("host", host)
        .header("api-key", "1");
    for (name, value) in extra_headers {
        request_builder = request_builder.header(*name, *value);
    }
    let request = request_builder.body(Empty::new()).unwrap();
    let response = sender.send_request(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    drop(sender);
    conn_task.await.unwrap();
    (status, headers, body)
}

async fn send_http1_request_without_api_key(
    addr: std::net::SocketAddr,
    method: hyper::Method,
    host: &str,
    path: &str,
) -> (StatusCode, hyper::HeaderMap, Bytes) {
    let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let (mut sender, conn) = hyper::client::conn::http1::Builder::new()
        .handshake::<_, Empty<Bytes>>(TokioIo::new(stream))
        .await
        .unwrap();

    let conn_task = tokio::spawn(async move {
        conn.await.unwrap();
    });

    let request = Request::builder()
        .uri(format!("http://{host}{path}"))
        .method(method)
        .header("host", host)
        .body(Empty::new())
        .unwrap();
    let response = sender.send_request(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    drop(sender);
    conn_task.await.unwrap();
    (status, headers, body)
}

async fn send_http1_requests_on_connection(
    addr: std::net::SocketAddr,
    request_count: usize,
) -> Vec<(StatusCode, Bytes)> {
    let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let (mut sender, conn) = hyper::client::conn::http1::Builder::new()
        .handshake::<_, Empty<Bytes>>(TokioIo::new(stream))
        .await
        .unwrap();

    let conn_task = tokio::spawn(async move {
        conn.await.unwrap();
    });

    let mut responses = Vec::with_capacity(request_count);
    for _ in 0..request_count {
        let request = Request::builder()
            .uri("http://gateway.local/")
            .header("host", "gateway.local")
            .header("api-key", "1")
            .body(Empty::new())
            .unwrap();
        let response = sender.send_request(request).await.unwrap();
        let status = response.status();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        responses.push((status, body));
    }

    drop(sender);
    conn_task.await.unwrap();
    responses
}

async fn send_h2c_request(addr: std::net::SocketAddr) -> (StatusCode, Bytes) {
    let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let (mut sender, conn) = hyper::client::conn::http2::Builder::new(TokioExecutor::new())
        .handshake::<_, Empty<Bytes>>(TokioIo::new(stream))
        .await
        .unwrap();

    let conn_task = tokio::spawn(async move {
        conn.await.unwrap();
    });

    let request = Request::builder()
        .uri("http://gateway.local/")
        .version(hyper::Version::HTTP_2)
        .header("api-key", "1")
        .body(Empty::new())
        .unwrap();
    let response = sender.send_request(request).await.unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    drop(sender);
    conn_task.await.unwrap();
    (status, body)
}

async fn send_https_http1_request(
    addr: std::net::SocketAddr,
    tls_fixture: &TlsFixture,
) -> (StatusCode, Bytes) {
    let mut root_store = rustls::RootCertStore::empty();
    root_store
        .add(CertificateDer::from(tls_fixture.cert_der.clone()))
        .expect("add gateway cert");
    let client_config = rustls::ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(Arc::new(client_config));
    let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let tls_stream = connector
        .connect(
            ServerName::try_from("localhost".to_string()).expect("localhost server name"),
            stream,
        )
        .await
        .unwrap();
    let (mut sender, conn) = hyper::client::conn::http1::Builder::new()
        .handshake::<_, Empty<Bytes>>(TokioIo::new(tls_stream))
        .await
        .unwrap();

    let conn_task = tokio::spawn(async move {
        conn.await.unwrap();
    });

    let request = Request::builder()
        .uri("https://gateway.local/")
        .header("api-key", "1")
        .body(Empty::new())
        .unwrap();
    let response = sender.send_request(request).await.unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    drop(sender);
    conn_task.await.unwrap();
    (status, body)
}

async fn send_https_h2_request(
    addr: std::net::SocketAddr,
    tls_fixture: &TlsFixture,
) -> (StatusCode, Bytes) {
    let mut root_store = rustls::RootCertStore::empty();
    root_store
        .add(CertificateDer::from(tls_fixture.cert_der.clone()))
        .expect("add gateway cert");
    let mut client_config = rustls::ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();
    client_config.alpn_protocols = vec![b"h2".to_vec()];
    let connector = tokio_rustls::TlsConnector::from(Arc::new(client_config));
    let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let tls_stream = connector
        .connect(
            ServerName::try_from("localhost".to_string()).expect("localhost server name"),
            stream,
        )
        .await
        .unwrap();
    let (mut sender, conn) = hyper::client::conn::http2::Builder::new(TokioExecutor::new())
        .handshake::<_, Empty<Bytes>>(TokioIo::new(tls_stream))
        .await
        .unwrap();

    let conn_task = tokio::spawn(async move {
        conn.await.unwrap();
    });

    let request = Request::builder()
        .uri("https://gateway.local/")
        .version(hyper::Version::HTTP_2)
        .header("api-key", "1")
        .body(Empty::new())
        .unwrap();
    let response = sender.send_request(request).await.unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    drop(sender);
    conn_task.await.unwrap();
    (status, body)
}

async fn spawn_tls_gateway_once(
    route_config: config::Route,
    db_path: PathBuf,
    tls_fixture: &TlsFixture,
) -> SpawnedGateway {
    let throttle_pool = throttle::init_throttle_at(&db_path).expect("throttle pool");
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    let server_tls = config::ServerTlsConfig {
        cert_path: Some(tls_fixture.cert_path.display().to_string()),
        key_path: Some(tls_fixture.key_path.display().to_string()),
    };
    let tls_acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(
        hyper_gateway::tls::build_server_config(&server_tls).expect("server tls config"),
    ));

    let task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let stream = tls_acceptor.accept(stream).await.unwrap();
        route::serve_connection(stream, Arc::from([route_config]), throttle_pool)
            .await
            .unwrap();
    });

    SpawnedGateway {
        addr,
        task: Some(task),
        db_guard: TempDbGuard::new(db_path),
    }
}

#[tokio::test]
async fn supports_http1_gateway_to_http1_upstream() {
    let upstream = spawn_http1_upstream().await;
    let gateway = spawn_gateway_once(
        config::Route {
            scheme: "http".to_string(),
            protocol: config::Protocol::Http1,
            authority: config::Authority {
                host: upstream.addr.ip().to_string(),
                port: upstream.addr.port().to_string(),
            },
            upstreams: Arc::from([]),
            tls: config::RouteTlsConfig::default(),
            timeout_ms: 30_000,
            retry_attempts: 0,
            circuit_breaker: None,
            hosts: Arc::from([]),
            methods: Arc::from([]),
            headers: BTreeMap::new(),
            queries: BTreeMap::new(),
            rewrite_prefix: None,
            request_header_filter: config::HeaderFilterConfig::default(),
            response_header_filter: config::HeaderFilterConfig::default(),
            path: "/".to_string(),
        },
        temp_db_path(),
    )
    .await;

    let (status, body) = send_http1_request(gateway.addr).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, Bytes::from_static(b"http1"));
    gateway.finish().await;
    upstream.finish().await;
}

#[tokio::test]
async fn serves_management_health_without_api_key_or_upstream() {
    let gateway = spawn_gateway_with_routes(Arc::from([]), temp_db_path()).await;

    let (status, headers, body) = send_http1_request_without_api_key(
        gateway.addr,
        hyper::Method::GET,
        "gateway.local",
        "/__gateway/health",
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers
            .get(hyper::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/json")
    );
    assert_eq!(body, Bytes::from_static(br#"{"status":"UP"}"#));
    gateway.finish().await;
}

#[tokio::test]
async fn serves_management_routes_without_api_key() {
    let gateway = spawn_gateway_with_routes(
        Arc::from([
            config::Route {
                scheme: "http".to_string(),
                protocol: config::Protocol::Http1,
                authority: config::Authority {
                    host: "127.0.0.1".to_string(),
                    port: "8001".to_string(),
                },
                upstreams: Arc::from([]),
                tls: config::RouteTlsConfig::default(),
                timeout_ms: 30_000,
                retry_attempts: 0,
                circuit_breaker: None,
                hosts: Arc::from([]),
                methods: Arc::from(["GET".to_string()]),
                headers: BTreeMap::new(),
                queries: BTreeMap::new(),
                rewrite_prefix: None,
                request_header_filter: config::HeaderFilterConfig::default(),
                response_header_filter: config::HeaderFilterConfig::default(),
                path: "/api".to_string(),
            },
            config::Route {
                scheme: "https".to_string(),
                protocol: config::Protocol::H2,
                authority: config::Authority {
                    host: "example.com".to_string(),
                    port: "443".to_string(),
                },
                upstreams: Arc::from([]),
                tls: config::RouteTlsConfig::default(),
                timeout_ms: 5_000,
                retry_attempts: 1,
                circuit_breaker: Some(config::CircuitBreakerConfig {
                    failure_threshold: 2,
                    open_ms: 250,
                }),
                hosts: Arc::from(["api.example.com".to_string()]),
                methods: Arc::from([]),
                headers: BTreeMap::from([("x-tenant".to_string(), "acme".to_string())]),
                queries: BTreeMap::new(),
                rewrite_prefix: Some("/internal".to_string()),
                request_header_filter: config::HeaderFilterConfig::default(),
                response_header_filter: config::HeaderFilterConfig::default(),
                path: "/admin".to_string(),
            },
        ]),
        temp_db_path(),
    )
    .await;

    let (status, headers, body) = send_http1_request_without_api_key(
        gateway.addr,
        hyper::Method::GET,
        "gateway.local",
        "/__gateway/routes",
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers
            .get(hyper::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/json")
    );
    let body = String::from_utf8(body.to_vec()).expect("utf8 json body");
    assert!(body.contains(r#""routes":["#));
    assert!(body.contains(r#""path":"/api""#));
    assert!(body.contains(r#""path":"/admin""#));
    assert!(body.contains(r#""protocol":"h2""#));
    assert!(body.contains(r#""failure_threshold":2"#));
    gateway.finish().await;
}

#[tokio::test]
async fn round_robins_across_configured_http1_upstreams() {
    let upstream_a = spawn_http1_upstream_with_body_for_connections(b"backend-a", 2).await;
    let upstream_b = spawn_http1_upstream_with_body_for_connections(b"backend-b", 1).await;
    let gateway = spawn_gateway_once(
        config::Route {
            scheme: "http".to_string(),
            protocol: config::Protocol::Http1,
            authority: config::Authority {
                host: upstream_a.addr.ip().to_string(),
                port: upstream_a.addr.port().to_string(),
            },
            upstreams: Arc::from([config::Authority {
                host: upstream_b.addr.ip().to_string(),
                port: upstream_b.addr.port().to_string(),
            }]),
            tls: config::RouteTlsConfig::default(),
            timeout_ms: 30_000,
            retry_attempts: 0,
            circuit_breaker: None,
            hosts: Arc::from([]),
            methods: Arc::from([]),
            headers: BTreeMap::new(),
            queries: BTreeMap::new(),
            rewrite_prefix: None,
            request_header_filter: config::HeaderFilterConfig::default(),
            response_header_filter: config::HeaderFilterConfig::default(),
            path: "/".to_string(),
        },
        temp_db_path(),
    )
    .await;

    let responses = send_http1_requests_on_connection(gateway.addr, 3).await;
    let (_, body_a) = &responses[0];
    let (_, body_b) = &responses[1];
    let (_, body_c) = &responses[2];

    assert_eq!(body_a, &Bytes::from_static(b"backend-a"));
    assert_eq!(body_b, &Bytes::from_static(b"backend-b"));
    assert_eq!(body_c, &Bytes::from_static(b"backend-a"));
    gateway.finish().await;
    upstream_a.finish().await;
    upstream_b.finish().await;
}

#[tokio::test]
async fn retries_failed_primary_connection_on_next_upstream() {
    let probe = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("probe listener");
    let dead_port = probe.local_addr().expect("probe addr").port();
    drop(probe);

    let backup = spawn_http1_upstream_with_body_for_connections(b"retry-backup", 1).await;
    let gateway = spawn_gateway_once(
        config::Route {
            scheme: "http".to_string(),
            protocol: config::Protocol::Http1,
            authority: config::Authority {
                host: "127.0.0.1".to_string(),
                port: dead_port.to_string(),
            },
            upstreams: Arc::from([config::Authority {
                host: backup.addr.ip().to_string(),
                port: backup.addr.port().to_string(),
            }]),
            tls: config::RouteTlsConfig::default(),
            timeout_ms: 30_000,
            retry_attempts: 1,
            circuit_breaker: None,
            hosts: Arc::from([]),
            methods: Arc::from([]),
            headers: BTreeMap::new(),
            queries: BTreeMap::new(),
            rewrite_prefix: None,
            request_header_filter: config::HeaderFilterConfig::default(),
            response_header_filter: config::HeaderFilterConfig::default(),
            path: "/".to_string(),
        },
        temp_db_path(),
    )
    .await;

    let (status, body) = send_http1_request(gateway.addr).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, Bytes::from_static(b"retry-backup"));
    gateway.finish().await;
    backup.finish().await;
}

#[tokio::test]
async fn retries_upstream_timeout_on_next_backend() {
    let slow = spawn_slow_http1_upstream(100).await;
    let backup = spawn_http1_upstream_with_body_for_connections(b"timeout-backup", 1).await;
    let gateway = spawn_gateway_once(
        config::Route {
            scheme: "http".to_string(),
            protocol: config::Protocol::Http1,
            authority: config::Authority {
                host: slow.addr.ip().to_string(),
                port: slow.addr.port().to_string(),
            },
            upstreams: Arc::from([config::Authority {
                host: backup.addr.ip().to_string(),
                port: backup.addr.port().to_string(),
            }]),
            tls: config::RouteTlsConfig::default(),
            timeout_ms: 10,
            retry_attempts: 1,
            circuit_breaker: None,
            hosts: Arc::from([]),
            methods: Arc::from([]),
            headers: BTreeMap::new(),
            queries: BTreeMap::new(),
            rewrite_prefix: None,
            request_header_filter: config::HeaderFilterConfig::default(),
            response_header_filter: config::HeaderFilterConfig::default(),
            path: "/".to_string(),
        },
        temp_db_path(),
    )
    .await;

    let (status, body) = send_http1_request(gateway.addr).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, Bytes::from_static(b"timeout-backup"));
    gateway.finish().await;
    slow.finish().await;
    backup.finish().await;
}

#[tokio::test]
async fn rewrites_request_prefix_before_forwarding_upstream() {
    let upstream = spawn_http1_upstream_echo_path().await;
    let gateway = spawn_gateway_once(
        config::Route {
            scheme: "http".to_string(),
            protocol: config::Protocol::Http1,
            authority: config::Authority {
                host: upstream.addr.ip().to_string(),
                port: upstream.addr.port().to_string(),
            },
            upstreams: Arc::from([]),
            tls: config::RouteTlsConfig::default(),
            timeout_ms: 30_000,
            retry_attempts: 0,
            circuit_breaker: None,
            hosts: Arc::from([]),
            methods: Arc::from([]),
            headers: BTreeMap::new(),
            queries: BTreeMap::new(),
            rewrite_prefix: Some("/".to_string()),
            request_header_filter: config::HeaderFilterConfig::default(),
            response_header_filter: config::HeaderFilterConfig::default(),
            path: "/api".to_string(),
        },
        temp_db_path(),
    )
    .await;

    let (status, body) = send_http1_request_with(
        gateway.addr,
        hyper::Method::GET,
        "gateway.local",
        "/api/users?id=7",
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, Bytes::from_static(b"/users?id=7"));
    gateway.finish().await;
    upstream.finish().await;
}

#[tokio::test]
async fn applies_request_header_filter_before_forwarding_upstream() {
    let upstream = spawn_http1_upstream_echo_filter_headers().await;
    let gateway = spawn_gateway_once(
        config::Route {
            scheme: "http".to_string(),
            protocol: config::Protocol::Http1,
            authority: config::Authority {
                host: upstream.addr.ip().to_string(),
                port: upstream.addr.port().to_string(),
            },
            upstreams: Arc::from([]),
            tls: config::RouteTlsConfig::default(),
            timeout_ms: 30_000,
            retry_attempts: 0,
            circuit_breaker: None,
            hosts: Arc::from([]),
            methods: Arc::from([]),
            headers: BTreeMap::new(),
            queries: BTreeMap::new(),
            rewrite_prefix: None,
            request_header_filter: config::HeaderFilterConfig {
                set: BTreeMap::from([("x-added-by-gateway".to_string(), "enabled".to_string())]),
                remove: Arc::from(["x-remove-me".to_string()]),
            },
            response_header_filter: config::HeaderFilterConfig::default(),
            path: "/".to_string(),
        },
        temp_db_path(),
    )
    .await;

    let (status, body) = send_http1_request_with_headers(
        gateway.addr,
        hyper::Method::GET,
        "gateway.local",
        "/",
        &[("x-remove-me", "client")],
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        Bytes::from_static(b"x-added-by-gateway=enabled;x-remove-me=false")
    );
    gateway.finish().await;
    upstream.finish().await;
}

#[tokio::test]
async fn applies_response_header_filter_before_returning_to_client() {
    let upstream = spawn_http1_upstream_with_response_headers().await;
    let gateway = spawn_gateway_once(
        config::Route {
            scheme: "http".to_string(),
            protocol: config::Protocol::Http1,
            authority: config::Authority {
                host: upstream.addr.ip().to_string(),
                port: upstream.addr.port().to_string(),
            },
            upstreams: Arc::from([]),
            tls: config::RouteTlsConfig::default(),
            timeout_ms: 30_000,
            retry_attempts: 0,
            circuit_breaker: None,
            hosts: Arc::from([]),
            methods: Arc::from([]),
            headers: BTreeMap::new(),
            queries: BTreeMap::new(),
            rewrite_prefix: None,
            request_header_filter: config::HeaderFilterConfig::default(),
            response_header_filter: config::HeaderFilterConfig {
                set: BTreeMap::from([("x-gateway".to_string(), "enabled".to_string())]),
                remove: Arc::from(["x-upstream-remove".to_string()]),
            },
            path: "/".to_string(),
        },
        temp_db_path(),
    )
    .await;

    let (status, headers, body) =
        send_http1_request_full(gateway.addr, hyper::Method::GET, "gateway.local", "/", &[]).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, Bytes::from_static(b"response-filter"));
    assert_eq!(headers.get("x-gateway").expect("gateway header"), "enabled");
    assert!(!headers.contains_key("x-upstream-remove"));
    assert_eq!(headers.get("x-upstream-keep").expect("keep header"), "ok");
    gateway.finish().await;
    upstream.finish().await;
}

#[tokio::test]
async fn selects_host_specific_route() {
    let default_upstream = spawn_http1_upstream_with_body_for_connections(b"default-host", 1).await;
    let api_upstream = spawn_http1_upstream_with_body_for_connections(b"api-host", 1).await;
    let gateway = spawn_gateway_with_routes(
        Arc::from([
            config::Route {
                scheme: "http".to_string(),
                protocol: config::Protocol::Http1,
                authority: config::Authority {
                    host: default_upstream.addr.ip().to_string(),
                    port: default_upstream.addr.port().to_string(),
                },
                upstreams: Arc::from([]),
                tls: config::RouteTlsConfig::default(),
                timeout_ms: 30_000,
                retry_attempts: 0,
                circuit_breaker: None,
                hosts: Arc::from([]),
                methods: Arc::from([]),
                headers: BTreeMap::new(),
                queries: BTreeMap::new(),
                rewrite_prefix: None,
                request_header_filter: config::HeaderFilterConfig::default(),
                response_header_filter: config::HeaderFilterConfig::default(),
                path: "/".to_string(),
            },
            config::Route {
                scheme: "http".to_string(),
                protocol: config::Protocol::Http1,
                authority: config::Authority {
                    host: api_upstream.addr.ip().to_string(),
                    port: api_upstream.addr.port().to_string(),
                },
                upstreams: Arc::from([]),
                tls: config::RouteTlsConfig::default(),
                timeout_ms: 30_000,
                retry_attempts: 0,
                circuit_breaker: None,
                hosts: Arc::from(["api.example.com".to_string()]),
                methods: Arc::from([]),
                headers: BTreeMap::new(),
                queries: BTreeMap::new(),
                rewrite_prefix: None,
                request_header_filter: config::HeaderFilterConfig::default(),
                response_header_filter: config::HeaderFilterConfig::default(),
                path: "/".to_string(),
            },
        ]),
        temp_db_path(),
    )
    .await;

    let (status, body) =
        send_http1_request_with(gateway.addr, hyper::Method::GET, "api.example.com", "/").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, Bytes::from_static(b"api-host"));
    gateway.finish().await;
    drop(default_upstream);
    api_upstream.finish().await;
}

#[tokio::test]
async fn selects_method_specific_route() {
    let default_upstream =
        spawn_http1_upstream_with_body_for_connections(b"default-method", 1).await;
    let post_upstream = spawn_http1_upstream_with_body_for_connections(b"post-method", 1).await;
    let gateway = spawn_gateway_with_routes(
        Arc::from([
            config::Route {
                scheme: "http".to_string(),
                protocol: config::Protocol::Http1,
                authority: config::Authority {
                    host: default_upstream.addr.ip().to_string(),
                    port: default_upstream.addr.port().to_string(),
                },
                upstreams: Arc::from([]),
                tls: config::RouteTlsConfig::default(),
                timeout_ms: 30_000,
                retry_attempts: 0,
                circuit_breaker: None,
                hosts: Arc::from([]),
                methods: Arc::from([]),
                headers: BTreeMap::new(),
                queries: BTreeMap::new(),
                rewrite_prefix: None,
                request_header_filter: config::HeaderFilterConfig::default(),
                response_header_filter: config::HeaderFilterConfig::default(),
                path: "/submit".to_string(),
            },
            config::Route {
                scheme: "http".to_string(),
                protocol: config::Protocol::Http1,
                authority: config::Authority {
                    host: post_upstream.addr.ip().to_string(),
                    port: post_upstream.addr.port().to_string(),
                },
                upstreams: Arc::from([]),
                tls: config::RouteTlsConfig::default(),
                timeout_ms: 30_000,
                retry_attempts: 0,
                circuit_breaker: None,
                hosts: Arc::from([]),
                methods: Arc::from(["POST".to_string()]),
                headers: BTreeMap::new(),
                queries: BTreeMap::new(),
                rewrite_prefix: None,
                request_header_filter: config::HeaderFilterConfig::default(),
                response_header_filter: config::HeaderFilterConfig::default(),
                path: "/submit".to_string(),
            },
        ]),
        temp_db_path(),
    )
    .await;

    let (status, body) = send_http1_request_with(
        gateway.addr,
        hyper::Method::POST,
        "gateway.local",
        "/submit",
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, Bytes::from_static(b"post-method"));
    gateway.finish().await;
    drop(default_upstream);
    post_upstream.finish().await;
}

#[tokio::test]
async fn selects_header_specific_route() {
    let default_upstream =
        spawn_http1_upstream_with_body_for_connections(b"default-header", 1).await;
    let header_upstream = spawn_http1_upstream_with_body_for_connections(b"header-match", 1).await;
    let gateway = spawn_gateway_with_routes(
        Arc::from([
            config::Route {
                scheme: "http".to_string(),
                protocol: config::Protocol::Http1,
                authority: config::Authority {
                    host: default_upstream.addr.ip().to_string(),
                    port: default_upstream.addr.port().to_string(),
                },
                upstreams: Arc::from([]),
                tls: config::RouteTlsConfig::default(),
                timeout_ms: 30_000,
                retry_attempts: 0,
                circuit_breaker: None,
                hosts: Arc::from([]),
                methods: Arc::from([]),
                headers: BTreeMap::new(),
                queries: BTreeMap::new(),
                rewrite_prefix: None,
                request_header_filter: config::HeaderFilterConfig::default(),
                response_header_filter: config::HeaderFilterConfig::default(),
                path: "/tenant".to_string(),
            },
            config::Route {
                scheme: "http".to_string(),
                protocol: config::Protocol::Http1,
                authority: config::Authority {
                    host: header_upstream.addr.ip().to_string(),
                    port: header_upstream.addr.port().to_string(),
                },
                upstreams: Arc::from([]),
                tls: config::RouteTlsConfig::default(),
                timeout_ms: 30_000,
                retry_attempts: 0,
                circuit_breaker: None,
                hosts: Arc::from([]),
                methods: Arc::from([]),
                headers: BTreeMap::from([("x-tenant".to_string(), "acme".to_string())]),
                queries: BTreeMap::new(),
                rewrite_prefix: None,
                request_header_filter: config::HeaderFilterConfig::default(),
                response_header_filter: config::HeaderFilterConfig::default(),
                path: "/tenant".to_string(),
            },
        ]),
        temp_db_path(),
    )
    .await;

    let (status, body) = send_http1_request_with_headers(
        gateway.addr,
        hyper::Method::GET,
        "gateway.local",
        "/tenant",
        &[("x-tenant", "acme")],
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, Bytes::from_static(b"header-match"));
    gateway.finish().await;
    drop(default_upstream);
    header_upstream.finish().await;
}

#[tokio::test]
async fn selects_query_specific_route() {
    let default_upstream =
        spawn_http1_upstream_with_body_for_connections(b"default-query", 1).await;
    let query_upstream = spawn_http1_upstream_with_body_for_connections(b"query-match", 1).await;
    let gateway = spawn_gateway_with_routes(
        Arc::from([
            config::Route {
                scheme: "http".to_string(),
                protocol: config::Protocol::Http1,
                authority: config::Authority {
                    host: default_upstream.addr.ip().to_string(),
                    port: default_upstream.addr.port().to_string(),
                },
                upstreams: Arc::from([]),
                tls: config::RouteTlsConfig::default(),
                timeout_ms: 30_000,
                retry_attempts: 0,
                circuit_breaker: None,
                hosts: Arc::from([]),
                methods: Arc::from([]),
                headers: BTreeMap::new(),
                queries: BTreeMap::new(),
                rewrite_prefix: None,
                request_header_filter: config::HeaderFilterConfig::default(),
                response_header_filter: config::HeaderFilterConfig::default(),
                path: "/search".to_string(),
            },
            config::Route {
                scheme: "http".to_string(),
                protocol: config::Protocol::Http1,
                authority: config::Authority {
                    host: query_upstream.addr.ip().to_string(),
                    port: query_upstream.addr.port().to_string(),
                },
                upstreams: Arc::from([]),
                tls: config::RouteTlsConfig::default(),
                timeout_ms: 30_000,
                retry_attempts: 0,
                circuit_breaker: None,
                hosts: Arc::from([]),
                methods: Arc::from([]),
                headers: BTreeMap::new(),
                queries: BTreeMap::from([("region".to_string(), "kr".to_string())]),
                rewrite_prefix: None,
                request_header_filter: config::HeaderFilterConfig::default(),
                response_header_filter: config::HeaderFilterConfig::default(),
                path: "/search".to_string(),
            },
        ]),
        temp_db_path(),
    )
    .await;

    let (status, body) = send_http1_request_with(
        gateway.addr,
        hyper::Method::GET,
        "gateway.local",
        "/search?region=kr",
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, Bytes::from_static(b"query-match"));
    gateway.finish().await;
    drop(default_upstream);
    query_upstream.finish().await;
}

#[tokio::test]
async fn supports_https_client_to_http1_upstream() {
    let tls_fixture = create_localhost_tls_fixture();
    let upstream = spawn_http1_upstream().await;
    let gateway = spawn_tls_gateway_once(
        config::Route {
            scheme: "http".to_string(),
            protocol: config::Protocol::Http1,
            authority: config::Authority {
                host: upstream.addr.ip().to_string(),
                port: upstream.addr.port().to_string(),
            },
            upstreams: Arc::from([]),
            tls: config::RouteTlsConfig::default(),
            timeout_ms: 30_000,
            retry_attempts: 0,
            circuit_breaker: None,
            hosts: Arc::from([]),
            methods: Arc::from([]),
            headers: BTreeMap::new(),
            queries: BTreeMap::new(),
            rewrite_prefix: None,
            request_header_filter: config::HeaderFilterConfig::default(),
            response_header_filter: config::HeaderFilterConfig::default(),
            path: "/".to_string(),
        },
        temp_db_path(),
        &tls_fixture,
    )
    .await;

    let (status, body) = send_https_http1_request(gateway.addr, &tls_fixture).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, Bytes::from_static(b"http1"));
    gateway.finish().await;
    upstream.finish().await;
}

#[tokio::test]
async fn supports_http1_gateway_to_https_h2_upstream() {
    let tls_fixture = create_localhost_tls_fixture();
    let upstream = spawn_https_h2_upstream(&tls_fixture).await;
    let gateway = spawn_gateway_once(
        config::Route {
            scheme: "https".to_string(),
            protocol: config::Protocol::H2,
            authority: config::Authority {
                host: "localhost".to_string(),
                port: upstream.addr.port().to_string(),
            },
            upstreams: Arc::from([]),
            tls: config::RouteTlsConfig {
                ca_cert_path: Some(tls_fixture.cert_path.display().to_string()),
            },
            timeout_ms: 30_000,
            retry_attempts: 0,
            circuit_breaker: None,
            hosts: Arc::from([]),
            methods: Arc::from([]),
            headers: BTreeMap::new(),
            queries: BTreeMap::new(),
            rewrite_prefix: None,
            request_header_filter: config::HeaderFilterConfig::default(),
            response_header_filter: config::HeaderFilterConfig::default(),
            path: "/".to_string(),
        },
        temp_db_path(),
    )
    .await;

    let (status, body) = send_http1_request(gateway.addr).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, Bytes::from_static(b"https-h2"));
    gateway.finish().await;
    upstream.finish().await;
}

#[tokio::test]
async fn supports_https_h2_client_to_http1_upstream() {
    let tls_fixture = create_localhost_tls_fixture();
    let upstream = spawn_http1_upstream().await;
    let gateway = spawn_tls_gateway_once(
        config::Route {
            scheme: "http".to_string(),
            protocol: config::Protocol::Http1,
            authority: config::Authority {
                host: upstream.addr.ip().to_string(),
                port: upstream.addr.port().to_string(),
            },
            upstreams: Arc::from([]),
            tls: config::RouteTlsConfig::default(),
            timeout_ms: 30_000,
            retry_attempts: 0,
            circuit_breaker: None,
            hosts: Arc::from([]),
            methods: Arc::from([]),
            headers: BTreeMap::new(),
            queries: BTreeMap::new(),
            rewrite_prefix: None,
            request_header_filter: config::HeaderFilterConfig::default(),
            response_header_filter: config::HeaderFilterConfig::default(),
            path: "/".to_string(),
        },
        temp_db_path(),
        &tls_fixture,
    )
    .await;

    let (status, body) = send_https_h2_request(gateway.addr, &tls_fixture).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, Bytes::from_static(b"http1"));
    gateway.finish().await;
    upstream.finish().await;
}

#[tokio::test]
async fn returns_gateway_timeout_when_upstream_is_too_slow() {
    let upstream = spawn_slow_http1_upstream(100).await;
    let gateway = spawn_gateway_once(
        config::Route {
            scheme: "http".to_string(),
            protocol: config::Protocol::Http1,
            authority: config::Authority {
                host: upstream.addr.ip().to_string(),
                port: upstream.addr.port().to_string(),
            },
            upstreams: Arc::from([]),
            tls: config::RouteTlsConfig::default(),
            timeout_ms: 10,
            retry_attempts: 0,
            circuit_breaker: None,
            hosts: Arc::from([]),
            methods: Arc::from([]),
            headers: BTreeMap::new(),
            queries: BTreeMap::new(),
            rewrite_prefix: None,
            request_header_filter: config::HeaderFilterConfig::default(),
            response_header_filter: config::HeaderFilterConfig::default(),
            path: "/".to_string(),
        },
        temp_db_path(),
    )
    .await;

    let (status, body) = send_http1_request(gateway.addr).await;
    assert_eq!(status, StatusCode::GATEWAY_TIMEOUT);
    assert!(body.is_empty());
    gateway.finish().await;
    upstream.finish().await;
}

#[tokio::test]
async fn opens_circuit_breaker_after_transport_failure_and_recovers_after_cooldown() {
    let upstream = spawn_http1_upstream_fail_once_then_succeed().await;
    let gateway = spawn_gateway_once(
        config::Route {
            scheme: "http".to_string(),
            protocol: config::Protocol::Http1,
            authority: config::Authority {
                host: upstream.addr.ip().to_string(),
                port: upstream.addr.port().to_string(),
            },
            upstreams: Arc::from([]),
            tls: config::RouteTlsConfig::default(),
            timeout_ms: 30_000,
            retry_attempts: 0,
            circuit_breaker: Some(config::CircuitBreakerConfig {
                failure_threshold: 1,
                open_ms: 250,
            }),
            hosts: Arc::from([]),
            methods: Arc::from([]),
            headers: BTreeMap::new(),
            queries: BTreeMap::new(),
            rewrite_prefix: None,
            request_header_filter: config::HeaderFilterConfig::default(),
            response_header_filter: config::HeaderFilterConfig::default(),
            path: "/breaker".to_string(),
        },
        temp_db_path(),
    )
    .await;

    let stream = tokio::net::TcpStream::connect(gateway.addr).await.unwrap();
    let (mut sender, conn) = hyper::client::conn::http1::Builder::new()
        .handshake::<_, Empty<Bytes>>(TokioIo::new(stream))
        .await
        .unwrap();

    let conn_task = tokio::spawn(async move {
        conn.await.unwrap();
    });

    let first_response = sender
        .send_request(
            Request::builder()
                .uri("http://gateway.local/breaker")
                .header("host", "gateway.local")
                .header("api-key", "1")
                .body(Empty::new())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(first_response.status(), StatusCode::BAD_GATEWAY);
    let first_body = first_response
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes();
    assert!(first_body.is_empty());

    let second_response = sender
        .send_request(
            Request::builder()
                .uri("http://gateway.local/breaker")
                .header("host", "gateway.local")
                .header("api-key", "1")
                .body(Empty::new())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(second_response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let second_body = second_response
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes();
    assert!(second_body.is_empty());

    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    let third_response = sender
        .send_request(
            Request::builder()
                .uri("http://gateway.local/breaker")
                .header("host", "gateway.local")
                .header("api-key", "1")
                .body(Empty::new())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(third_response.status(), StatusCode::OK);
    let third_body = third_response
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes();
    assert_eq!(third_body, Bytes::from_static(b"breaker-recovered"));

    drop(sender);
    conn_task.await.unwrap();
    gateway.finish().await;
    upstream.finish().await;
}

#[tokio::test]
async fn supports_http1_gateway_to_http1_hostname_upstream() {
    let upstream = spawn_http1_upstream().await;
    let gateway = spawn_gateway_once(
        config::Route {
            scheme: "http".to_string(),
            protocol: config::Protocol::Http1,
            authority: config::Authority {
                host: "localhost".to_string(),
                port: upstream.addr.port().to_string(),
            },
            upstreams: Arc::from([]),
            tls: config::RouteTlsConfig::default(),
            timeout_ms: 30_000,
            retry_attempts: 0,
            circuit_breaker: None,
            hosts: Arc::from([]),
            methods: Arc::from([]),
            headers: BTreeMap::new(),
            queries: BTreeMap::new(),
            rewrite_prefix: None,
            request_header_filter: config::HeaderFilterConfig::default(),
            response_header_filter: config::HeaderFilterConfig::default(),
            path: "/".to_string(),
        },
        temp_db_path(),
    )
    .await;

    let (status, body) = send_http1_request(gateway.addr).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, Bytes::from_static(b"http1"));
    gateway.finish().await;
    upstream.finish().await;
}

#[tokio::test]
async fn supports_http1_gateway_to_https_upstream() {
    let tls_fixture = create_localhost_tls_fixture();
    let upstream = spawn_https_http1_upstream(&tls_fixture).await;
    let gateway = spawn_gateway_once(
        config::Route {
            scheme: "https".to_string(),
            protocol: config::Protocol::Http1,
            authority: config::Authority {
                host: "localhost".to_string(),
                port: upstream.addr.port().to_string(),
            },
            upstreams: Arc::from([]),
            tls: config::RouteTlsConfig {
                ca_cert_path: Some(tls_fixture.cert_path.display().to_string()),
            },
            timeout_ms: 30_000,
            retry_attempts: 0,
            circuit_breaker: None,
            hosts: Arc::from([]),
            methods: Arc::from([]),
            headers: BTreeMap::new(),
            queries: BTreeMap::new(),
            rewrite_prefix: None,
            request_header_filter: config::HeaderFilterConfig::default(),
            response_header_filter: config::HeaderFilterConfig::default(),
            path: "/".to_string(),
        },
        temp_db_path(),
    )
    .await;

    let (status, body) = send_http1_request(gateway.addr).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, Bytes::from_static(b"https-http1"));
    gateway.finish().await;
    upstream.finish().await;
}

#[tokio::test]
async fn returns_bad_gateway_when_upstream_connect_fails() {
    let probe = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("probe listener");
    let unused_port = probe.local_addr().expect("probe addr").port();
    drop(probe);

    let gateway = spawn_gateway_once(
        config::Route {
            scheme: "http".to_string(),
            protocol: config::Protocol::Http1,
            authority: config::Authority {
                host: "127.0.0.1".to_string(),
                port: unused_port.to_string(),
            },
            upstreams: Arc::from([]),
            tls: config::RouteTlsConfig::default(),
            timeout_ms: 30_000,
            retry_attempts: 0,
            circuit_breaker: None,
            hosts: Arc::from([]),
            methods: Arc::from([]),
            headers: BTreeMap::new(),
            queries: BTreeMap::new(),
            rewrite_prefix: None,
            request_header_filter: config::HeaderFilterConfig::default(),
            response_header_filter: config::HeaderFilterConfig::default(),
            path: "/".to_string(),
        },
        temp_db_path(),
    )
    .await;

    let (status, body) = send_http1_request(gateway.addr).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert!(body.is_empty());
    gateway.finish().await;
}

#[tokio::test]
async fn supports_h2c_gateway_to_h2c_upstream() {
    let upstream = spawn_h2c_upstream().await;
    let gateway = spawn_gateway_once(
        config::Route {
            scheme: "http".to_string(),
            protocol: config::Protocol::H2c,
            authority: config::Authority {
                host: upstream.addr.ip().to_string(),
                port: upstream.addr.port().to_string(),
            },
            upstreams: Arc::from([]),
            tls: config::RouteTlsConfig::default(),
            timeout_ms: 30_000,
            retry_attempts: 0,
            circuit_breaker: None,
            hosts: Arc::from([]),
            methods: Arc::from([]),
            headers: BTreeMap::new(),
            queries: BTreeMap::new(),
            rewrite_prefix: None,
            request_header_filter: config::HeaderFilterConfig::default(),
            response_header_filter: config::HeaderFilterConfig::default(),
            path: "/".to_string(),
        },
        temp_db_path(),
    )
    .await;

    let (status, body) = send_h2c_request(gateway.addr).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, Bytes::from_static(b"h2c"));
    gateway.finish().await;
    upstream.finish().await;
}

#[tokio::test]
async fn supports_h2c_client_to_http1_upstream() {
    let upstream = spawn_http1_upstream().await;
    let gateway = spawn_gateway_once(
        config::Route {
            scheme: "http".to_string(),
            protocol: config::Protocol::Http1,
            authority: config::Authority {
                host: upstream.addr.ip().to_string(),
                port: upstream.addr.port().to_string(),
            },
            upstreams: Arc::from([]),
            tls: config::RouteTlsConfig::default(),
            timeout_ms: 30_000,
            retry_attempts: 0,
            circuit_breaker: None,
            hosts: Arc::from([]),
            methods: Arc::from([]),
            headers: BTreeMap::new(),
            queries: BTreeMap::new(),
            rewrite_prefix: None,
            request_header_filter: config::HeaderFilterConfig::default(),
            response_header_filter: config::HeaderFilterConfig::default(),
            path: "/".to_string(),
        },
        temp_db_path(),
    )
    .await;

    let (status, body) = send_h2c_request(gateway.addr).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, Bytes::from_static(b"http1"));
    gateway.finish().await;
    upstream.finish().await;
}

#[tokio::test]
async fn supports_http1_client_to_h2c_upstream() {
    let upstream = spawn_h2c_upstream().await;
    let gateway = spawn_gateway_once(
        config::Route {
            scheme: "http".to_string(),
            protocol: config::Protocol::H2c,
            authority: config::Authority {
                host: upstream.addr.ip().to_string(),
                port: upstream.addr.port().to_string(),
            },
            upstreams: Arc::from([]),
            tls: config::RouteTlsConfig::default(),
            timeout_ms: 30_000,
            retry_attempts: 0,
            circuit_breaker: None,
            hosts: Arc::from([]),
            methods: Arc::from([]),
            headers: BTreeMap::new(),
            queries: BTreeMap::new(),
            rewrite_prefix: None,
            request_header_filter: config::HeaderFilterConfig::default(),
            response_header_filter: config::HeaderFilterConfig::default(),
            path: "/".to_string(),
        },
        temp_db_path(),
    )
    .await;

    let (status, body) = send_http1_request(gateway.addr).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, Bytes::from_static(b"h2c"));
    gateway.finish().await;
    upstream.finish().await;
}

#[tokio::test]
async fn supports_multi_route_protocol_selection() {
    let default_upstream = spawn_http1_upstream().await;
    let h2c_upstream = spawn_h2c_upstream().await;
    let gateway = spawn_gateway_with_routes(
        Arc::from([
            config::Route {
                scheme: "http".to_string(),
                protocol: config::Protocol::Http1,
                authority: config::Authority {
                    host: default_upstream.addr.ip().to_string(),
                    port: default_upstream.addr.port().to_string(),
                },
                upstreams: Arc::from([]),
                tls: config::RouteTlsConfig::default(),
                timeout_ms: 30_000,
                retry_attempts: 0,
                circuit_breaker: None,
                hosts: Arc::from([]),
                methods: Arc::from([]),
                headers: BTreeMap::new(),
                queries: BTreeMap::new(),
                rewrite_prefix: None,
                request_header_filter: config::HeaderFilterConfig::default(),
                response_header_filter: config::HeaderFilterConfig::default(),
                path: "/".to_string(),
            },
            config::Route {
                scheme: "http".to_string(),
                protocol: config::Protocol::H2c,
                authority: config::Authority {
                    host: h2c_upstream.addr.ip().to_string(),
                    port: h2c_upstream.addr.port().to_string(),
                },
                upstreams: Arc::from([]),
                tls: config::RouteTlsConfig::default(),
                timeout_ms: 30_000,
                retry_attempts: 0,
                circuit_breaker: None,
                hosts: Arc::from([]),
                methods: Arc::from([]),
                headers: BTreeMap::new(),
                queries: BTreeMap::new(),
                rewrite_prefix: None,
                request_header_filter: config::HeaderFilterConfig::default(),
                response_header_filter: config::HeaderFilterConfig::default(),
                path: "/h2".to_string(),
            },
        ]),
        temp_db_path(),
    )
    .await;

    let stream = tokio::net::TcpStream::connect(gateway.addr).await.unwrap();
    let (mut sender, conn) = hyper::client::conn::http1::Builder::new()
        .handshake::<_, Empty<Bytes>>(TokioIo::new(stream))
        .await
        .unwrap();

    let conn_task = tokio::spawn(async move {
        conn.await.unwrap();
    });

    let request = Request::builder()
        .uri("http://gateway.local/h2")
        .header("api-key", "1")
        .body(Empty::new())
        .unwrap();
    let response = sender.send_request(request).await.unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    drop(sender);
    conn_task.await.unwrap();

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, Bytes::from_static(b"h2c"));
    gateway.finish().await;
    h2c_upstream.finish().await;
    drop(default_upstream);
}

#[test]
fn temp_db_guard_removes_artifacts_during_panic_unwind() {
    let db_path = temp_db_path();
    for path in [
        db_path.clone(),
        PathBuf::from(format!("{}-journal", db_path.display())),
        PathBuf::from(format!("{}-shm", db_path.display())),
        PathBuf::from(format!("{}-wal", db_path.display())),
    ] {
        std::fs::write(&path, b"temp").unwrap();
    }

    let unwind = std::panic::catch_unwind({
        let db_path = db_path.clone();
        move || {
            let _guard = TempDbGuard::new(db_path);
            panic!("intentional panic for cleanup regression");
        }
    });

    assert!(unwind.is_err());
    assert_temp_db_artifacts_removed(&db_path);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dropped_gateway_eventually_removes_temp_db_artifacts() {
    let upstream = spawn_http1_upstream().await;
    let db_path = temp_db_path();
    let gateway = spawn_gateway_once(
        config::Route {
            scheme: "http".to_string(),
            protocol: config::Protocol::Http1,
            authority: config::Authority {
                host: upstream.addr.ip().to_string(),
                port: upstream.addr.port().to_string(),
            },
            upstreams: Arc::from([]),
            tls: config::RouteTlsConfig::default(),
            timeout_ms: 30_000,
            retry_attempts: 0,
            circuit_breaker: None,
            hosts: Arc::from([]),
            methods: Arc::from([]),
            headers: BTreeMap::new(),
            queries: BTreeMap::new(),
            rewrite_prefix: None,
            request_header_filter: config::HeaderFilterConfig::default(),
            response_header_filter: config::HeaderFilterConfig::default(),
            path: "/".to_string(),
        },
        db_path.clone(),
    )
    .await;

    let stream = tokio::net::TcpStream::connect(gateway.addr).await.unwrap();
    tokio::task::yield_now().await;
    drop(gateway);
    drop(stream);

    wait_for_temp_db_artifacts_removed(&db_path);
    drop(upstream);
}
