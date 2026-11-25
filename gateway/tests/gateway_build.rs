use http_body_util::{BodyExt, Empty, Full};
use hyper::{body::Bytes, Request, Response, StatusCode};
use hyper_gateway::{
    config, config::Protocol, discovery, entity, logger, metrics, route, throttle, trace,
};
use hyper_util::rt::{TokioExecutor, TokioIo};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName};
use std::{
    collections::BTreeMap,
    convert::TryFrom,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
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

struct SpawnedDedicatedManagementGateway {
    gateway_addr: std::net::SocketAddr,
    management_addr: std::net::SocketAddr,
    gateway_task: Option<JoinHandle<()>>,
    management_task: Option<JoinHandle<()>>,
    db_guard: TempDbGuard,
}

impl SpawnedGateway {
    async fn finish(mut self) {
        if let Some(task) = self.task.take() {
            if task.is_finished() {
                task.await.unwrap();
            } else {
                task.abort();
                let _ = task.await;
            }
        }
        let db_path = self.db_guard.path().clone();
        drop(self);
        assert_temp_db_artifacts_removed(&db_path);
    }
}

impl SpawnedDedicatedManagementGateway {
    async fn finish(mut self) {
        if let Some(task) = self.gateway_task.take() {
            if task.is_finished() {
                task.await.unwrap();
            } else {
                task.abort();
                let _ = task.await;
            }
        }
        if let Some(task) = self.management_task.take() {
            if task.is_finished() {
                task.await.unwrap();
            } else {
                task.abort();
                let _ = task.await;
            }
        }
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

#[derive(Clone)]
struct RegistryInstance {
    host: String,
    port: u16,
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

impl Drop for SpawnedDedicatedManagementGateway {
    fn drop(&mut self) {
        if let Some(task) = self.gateway_task.take() {
            task.abort();
        }
        if let Some(task) = self.management_task.take() {
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

async fn spawn_gateway_with_gateway_config(
    gateway_config: config::GatewayConfig,
    db_path: PathBuf,
) -> SpawnedGateway {
    spawn_gateway_with_settings(
        gateway_config.routes,
        db_path,
        gateway_config.server.management,
        gateway_config.server.discovery,
    )
    .await
}

async fn spawn_gateway_with_routes(
    routes: Arc<[config::Route]>,
    db_path: PathBuf,
) -> SpawnedGateway {
    spawn_gateway_with_settings(
        routes,
        db_path,
        config::ManagementConfig::default(),
        config::DiscoveryConfig::default(),
    )
    .await
}

async fn spawn_gateway_with_settings(
    routes: Arc<[config::Route]>,
    db_path: PathBuf,
    management_config: config::ManagementConfig,
    discovery_config: config::DiscoveryConfig,
) -> SpawnedGateway {
    let throttle_pool = throttle::init_throttle_at(&db_path).expect("throttle pool");
    let metrics_collector = metrics::new_collector();
    let discovery_registry = discovery::init(&discovery_config, &routes)
        .await
        .expect("discovery registry");
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();

    let task = tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let routes = routes.clone();
            let throttle_pool = throttle_pool.clone();
            let metrics_collector = metrics_collector.clone();
            let management_config = management_config.clone();
            let discovery_registry = discovery_registry.clone();

            tokio::spawn(async move {
                route::run(
                    stream,
                    routes,
                    throttle_pool,
                    metrics_collector,
                    management_config,
                    discovery_registry,
                )
                .await
                .unwrap();
            });
        }
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

async fn spawn_http1_upstream_with_status_sequence(statuses: Vec<StatusCode>) -> SpawnedUpstream {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();

    let task = tokio::spawn(async move {
        for status in statuses {
            let (stream, _) = listener.accept().await.unwrap();
            let service = hyper::service::service_fn(
                move |_request: Request<hyper::body::Incoming>| async move {
                    Ok::<_, hyper::Error>(
                        Response::builder()
                            .status(status)
                            .body(Full::new(Bytes::from(status.as_str().as_bytes().to_vec())))
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

async fn spawn_http1_upstream_echo_content_type_and_body(
    connection_count: usize,
) -> SpawnedUpstream {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();

    let task = tokio::spawn(async move {
        for _ in 0..connection_count {
            let (stream, _) = listener.accept().await.unwrap();
            let service =
                hyper::service::service_fn(|request: Request<hyper::body::Incoming>| async move {
                    let content_type = request
                        .headers()
                        .get(hyper::header::CONTENT_TYPE)
                        .and_then(|value| value.to_str().ok())
                        .unwrap_or("")
                        .to_string();
                    let body = request.into_body().collect().await.unwrap().to_bytes();
                    let mut echoed = content_type.as_bytes().to_vec();
                    echoed.push(b'\n');
                    echoed.extend_from_slice(&body);

                    Ok::<_, hyper::Error>(
                        Response::builder()
                            .status(StatusCode::OK)
                            .body(Full::new(Bytes::from(echoed)))
                            .unwrap(),
                    )
                });

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

async fn spawn_http1_upstream_signal_first_body_chunk(
) -> (SpawnedUpstream, tokio::sync::oneshot::Receiver<Bytes>) {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    let (chunk_tx, chunk_rx) = tokio::sync::oneshot::channel();
    let chunk_tx = Arc::new(std::sync::Mutex::new(Some(chunk_tx)));

    let task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let service = hyper::service::service_fn(move |request: Request<hyper::body::Incoming>| {
            let chunk_tx = chunk_tx.clone();
            async move {
                let mut body = request.into_body();
                let first_chunk = match body.frame().await {
                    Some(Ok(frame)) => frame
                        .data_ref()
                        .map(|data| data.clone())
                        .unwrap_or_else(Bytes::new),
                    Some(Err(err)) => return Err::<Response<Full<Bytes>>, _>(err),
                    None => Bytes::new(),
                };

                if let Some(sender) = chunk_tx.lock().unwrap().take() {
                    let _ = sender.send(first_chunk.clone());
                }

                let _ = body.collect().await?;

                Ok::<_, hyper::Error>(
                    Response::builder()
                        .status(StatusCode::OK)
                        .body(Full::new(Bytes::from_static(b"streamed")))
                        .unwrap(),
                )
            }
        });

        hyper::server::conn::http1::Builder::new()
            .serve_connection(TokioIo::new(stream), service)
            .await
            .ok();
    });

    (
        SpawnedUpstream {
            addr,
            task: Some(task),
        },
        chunk_rx,
    )
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

async fn spawn_consul_registry(
    service: &str,
    instances: Arc<std::sync::Mutex<Vec<RegistryInstance>>>,
) -> SpawnedUpstream {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    let service = service.to_string();

    let task = tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let service = service.clone();
            let instances = instances.clone();

            tokio::spawn(async move {
                let svc = hyper::service::service_fn(
                    move |request: Request<hyper::body::Incoming>| {
                        let service = service.clone();
                        let instances = instances.clone();
                        async move {
                            let expected_path = format!("/v1/health/service/{}", service);
                            if request.uri().path() != expected_path {
                                return Ok::<_, hyper::Error>(
                                    Response::builder()
                                        .status(StatusCode::NOT_FOUND)
                                        .body(Full::new(Bytes::new()))
                                        .unwrap(),
                                );
                            }

                            let payload = instances
                                .lock()
                                .unwrap()
                                .iter()
                                .map(|instance| {
                                    serde_json::json!({
                                        "Node": { "Address": instance.host },
                                        "Service": { "Address": instance.host, "Port": instance.port }
                                    })
                                })
                                .collect::<Vec<_>>();

                            Ok::<_, hyper::Error>(
                                Response::builder()
                                    .status(StatusCode::OK)
                                    .header(hyper::header::CONTENT_TYPE, "application/json")
                                    .body(Full::new(Bytes::from(
                                        serde_json::to_vec(&payload).unwrap(),
                                    )))
                                    .unwrap(),
                            )
                        }
                    },
                );

                hyper::server::conn::http1::Builder::new()
                    .serve_connection(TokioIo::new(stream), svc)
                    .await
                    .ok();
            });
        }
    });

    SpawnedUpstream {
        addr,
        task: Some(task),
    }
}

async fn spawn_eureka_registry(
    service: &str,
    instances: Arc<std::sync::Mutex<Vec<RegistryInstance>>>,
) -> SpawnedUpstream {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    let service = service.to_ascii_uppercase();

    let task = tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let service = service.clone();
            let instances = instances.clone();

            tokio::spawn(async move {
                let svc =
                    hyper::service::service_fn(move |request: Request<hyper::body::Incoming>| {
                        let service = service.clone();
                        let instances = instances.clone();
                        async move {
                            let expected_path = format!("/eureka/v2/apps/{}", service);
                            if request.uri().path() != expected_path {
                                return Ok::<_, hyper::Error>(
                                    Response::builder()
                                        .status(StatusCode::NOT_FOUND)
                                        .body(Full::new(Bytes::new()))
                                        .unwrap(),
                                );
                            }

                            let payload = serde_json::json!({
                                "application": {
                                    "instance": instances.lock().unwrap().iter().map(|instance| {
                                        serde_json::json!({
                                            "hostName": "localhost",
                                            "ipAddr": instance.host,
                                            "status": "UP",
                                            "port": { "$": instance.port, "@enabled": "true" },
                                            "securePort": { "$": 8443, "@enabled": "false" }
                                        })
                                    }).collect::<Vec<_>>()
                                }
                            });

                            Ok::<_, hyper::Error>(
                                Response::builder()
                                    .status(StatusCode::OK)
                                    .header(hyper::header::CONTENT_TYPE, "application/json")
                                    .body(Full::new(Bytes::from(
                                        serde_json::to_vec(&payload).unwrap(),
                                    )))
                                    .unwrap(),
                            )
                        }
                    });

                hyper::server::conn::http1::Builder::new()
                    .serve_connection(TokioIo::new(stream), svc)
                    .await
                    .ok();
            });
        }
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

async fn send_http1_request_with_body(
    addr: std::net::SocketAddr,
    method: hyper::Method,
    host: &str,
    path: &str,
    extra_headers: &[(&str, &str)],
    body: Bytes,
) -> (StatusCode, Bytes) {
    let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let (mut sender, conn) = hyper::client::conn::http1::Builder::new()
        .handshake::<_, Full<Bytes>>(TokioIo::new(stream))
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
    let request = request_builder.body(Full::new(body)).unwrap();
    let response = sender.send_request(request).await.unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    drop(sender);
    conn_task.abort();
    let _ = conn_task.await;
    (status, body)
}

async fn send_http1_request_without_api_key(
    addr: std::net::SocketAddr,
    method: hyper::Method,
    host: &str,
    path: &str,
) -> (StatusCode, hyper::HeaderMap, Bytes) {
    send_http1_request_without_api_key_with_headers(addr, method, host, path, &[]).await
}

async fn send_http1_request_without_api_key_with_headers(
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
        .header("host", host);
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

struct Http1RequestSpec<'a> {
    method: hyper::Method,
    host: &'a str,
    path: &'a str,
    include_api_key: bool,
}

async fn send_http1_request_sequence(
    addr: std::net::SocketAddr,
    requests: &[Http1RequestSpec<'_>],
) -> Vec<(StatusCode, hyper::HeaderMap, Bytes)> {
    let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let (mut sender, conn) = hyper::client::conn::http1::Builder::new()
        .handshake::<_, Empty<Bytes>>(TokioIo::new(stream))
        .await
        .unwrap();

    let conn_task = tokio::spawn(async move {
        conn.await.unwrap();
    });

    let mut responses = Vec::with_capacity(requests.len());
    for request_spec in requests {
        let mut request_builder = Request::builder()
            .uri(format!("http://{}{}", request_spec.host, request_spec.path))
            .method(request_spec.method.clone())
            .header("host", request_spec.host);
        if request_spec.include_api_key {
            request_builder = request_builder.header("api-key", "1");
        }
        let request = request_builder.body(Empty::new()).unwrap();
        let response = sender.send_request(request).await.unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        responses.push((status, headers, body));
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
    let metrics_collector = metrics::new_collector();
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    let server_tls = config::ServerTlsConfig {
        cert_path: Some(tls_fixture.cert_path.display().to_string()),
        key_path: Some(tls_fixture.key_path.display().to_string()),
    };
    let discovery_registry =
        discovery::init(&config::DiscoveryConfig::default(), &[route_config.clone()])
            .await
            .expect("discovery registry");
    let tls_acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(
        hyper_gateway::tls::build_server_config(&server_tls).expect("server tls config"),
    ));

    let task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let stream = tls_acceptor.accept(stream).await.unwrap();
        route::serve_connection(
            stream,
            Arc::from([route_config]),
            throttle_pool,
            metrics_collector,
            config::ManagementConfig::default(),
            discovery_registry,
        )
        .await
        .unwrap();
    });

    SpawnedGateway {
        addr,
        task: Some(task),
        db_guard: TempDbGuard::new(db_path),
    }
}

async fn spawn_gateway_with_management_config(
    routes: Arc<[config::Route]>,
    db_path: PathBuf,
    management_config: config::ManagementConfig,
) -> SpawnedGateway {
    spawn_gateway_with_settings(
        routes,
        db_path,
        management_config,
        config::DiscoveryConfig::default(),
    )
    .await
}

async fn spawn_gateway_with_dedicated_management_listener(
    routes: Arc<[config::Route]>,
    db_path: PathBuf,
    management_config: config::ManagementConfig,
) -> SpawnedDedicatedManagementGateway {
    let throttle_pool = throttle::init_throttle_at(&db_path).expect("throttle pool");
    let metrics_collector = metrics::new_collector();
    let discovery_registry = discovery::init(&config::DiscoveryConfig::default(), &routes)
        .await
        .expect("discovery registry");
    let gateway_listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let gateway_addr = gateway_listener.local_addr().unwrap();
    let management_listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let management_addr = management_listener.local_addr().unwrap();

    let gateway_routes = routes.clone();
    let gateway_metrics = metrics_collector.clone();
    let gateway_discovery = discovery_registry.clone();
    let gateway_task = tokio::spawn(async move {
        let (stream, _) = gateway_listener.accept().await.unwrap();
        route::run_proxy_only(
            stream,
            gateway_routes,
            throttle_pool,
            gateway_metrics,
            gateway_discovery,
        )
        .await
        .unwrap();
    });

    let management_routes = routes.clone();
    let management_metrics = metrics_collector.clone();
    let management_task = tokio::spawn(async move {
        let (stream, _) = management_listener.accept().await.unwrap();
        hyper_gateway::management::run(
            stream,
            management_routes,
            management_metrics,
            management_config,
        )
        .await
        .unwrap();
    });

    SpawnedDedicatedManagementGateway {
        gateway_addr,
        management_addr,
        gateway_task: Some(gateway_task),
        management_task: Some(management_task),
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
            discovery: None,
            upstreams: Arc::from([]),
            tls: config::RouteTlsConfig::default(),
            timeout_ms: 30_000,
            retry_attempts: 0,
            retry_statuses: Arc::from([]),
            retry_backoff_ms: 0,
            circuit_breaker: None,
            rate_limit: None,
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
async fn allows_unlimited_route_without_api_key() {
    let upstream = spawn_http1_upstream().await;
    let gateway = spawn_gateway_once(
        config::Route {
            scheme: "http".to_string(),
            protocol: config::Protocol::Http1,
            authority: config::Authority {
                host: upstream.addr.ip().to_string(),
                port: upstream.addr.port().to_string(),
            },
            discovery: None,
            upstreams: Arc::from([]),
            tls: config::RouteTlsConfig::default(),
            timeout_ms: 30_000,
            retry_attempts: 0,
            retry_statuses: Arc::from([]),
            retry_backoff_ms: 0,
            circuit_breaker: None,
            rate_limit: None,
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

    let (status, _, body) =
        send_http1_request_without_api_key(gateway.addr, hyper::Method::GET, "gateway.local", "/")
            .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, Bytes::from_static(b"http1"));

    gateway.finish().await;
    upstream.finish().await;
}

#[tokio::test]
async fn rejects_rate_limited_route_without_key_header() {
    let upstream = spawn_http1_upstream().await;
    let gateway = spawn_gateway_once(
        config::Route {
            scheme: "http".to_string(),
            protocol: config::Protocol::Http1,
            authority: config::Authority {
                host: upstream.addr.ip().to_string(),
                port: upstream.addr.port().to_string(),
            },
            discovery: None,
            upstreams: Arc::from([]),
            tls: config::RouteTlsConfig::default(),
            timeout_ms: 30_000,
            retry_attempts: 0,
            retry_statuses: Arc::from([]),
            retry_backoff_ms: 0,
            circuit_breaker: None,
            rate_limit: Some(config::RateLimitConfig {
                requests: 1,
                window_ms: 1_000,
                key_header: "api-key".to_string(),
            }),
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

    let (status, _, body) =
        send_http1_request_without_api_key(gateway.addr, hyper::Method::GET, "gateway.local", "/")
            .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body.is_empty());

    gateway.finish().await;
    drop(upstream);
}

#[tokio::test]
async fn returns_too_many_requests_when_route_rate_limit_is_exceeded() {
    let upstream = spawn_http1_upstream_with_body_for_connections(b"rate-limited", 1).await;
    let gateway = spawn_gateway_once(
        config::Route {
            scheme: "http".to_string(),
            protocol: config::Protocol::Http1,
            authority: config::Authority {
                host: upstream.addr.ip().to_string(),
                port: upstream.addr.port().to_string(),
            },
            discovery: None,
            upstreams: Arc::from([]),
            tls: config::RouteTlsConfig::default(),
            timeout_ms: 30_000,
            retry_attempts: 0,
            retry_statuses: Arc::from([]),
            retry_backoff_ms: 0,
            circuit_breaker: None,
            rate_limit: Some(config::RateLimitConfig {
                requests: 1,
                window_ms: 1_000,
                key_header: "api-key".to_string(),
            }),
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

    let responses = send_http1_request_sequence(
        gateway.addr,
        &[
            Http1RequestSpec {
                method: hyper::Method::GET,
                host: "gateway.local",
                path: "/",
                include_api_key: true,
            },
            Http1RequestSpec {
                method: hyper::Method::GET,
                host: "gateway.local",
                path: "/",
                include_api_key: true,
            },
        ],
    )
    .await;

    assert_eq!(responses[0].0, StatusCode::OK);
    assert_eq!(responses[0].2, Bytes::from_static(b"rate-limited"));
    assert_eq!(responses[1].0, StatusCode::TOO_MANY_REQUESTS);
    assert!(responses[1].2.is_empty());

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
async fn proxies_scg_style_route_definition() {
    let upstream = spawn_http1_upstream().await;
    let config = serde_yaml::from_str::<config::GatewayConfig>(&format!(
        r#"
spring:
  cloud:
    gateway:
      routes:
        - id: products
          uri: http://localhost:{port}
          predicates:
            - Path=/products/**
            - Method=GET
"#,
        port = upstream.addr.port()
    ))
    .expect("scg config should parse");
    let gateway = spawn_gateway_with_routes(config.routes, temp_db_path()).await;

    let (status, body) = send_http1_request_with(
        gateway.addr,
        hyper::Method::GET,
        "gateway.local",
        "/products",
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, Bytes::from_static(b"http1"));

    gateway.finish().await;
    upstream.finish().await;
}

#[tokio::test]
async fn rewrites_scg_style_strip_prefix_before_forwarding() {
    let upstream = spawn_http1_upstream_echo_path().await;
    let config = serde_yaml::from_str::<config::GatewayConfig>(&format!(
        r#"
spring:
  cloud:
    gateway:
      routes:
        - id: users
          uri: http://localhost:{port}
          predicates:
            - Path=/api/**
          filters:
            - StripPrefix=1
"#,
        port = upstream.addr.port()
    ))
    .expect("scg config should parse");
    let gateway = spawn_gateway_with_routes(config.routes, temp_db_path()).await;

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
async fn proxies_scg_style_lb_route_definition() {
    let upstream = spawn_http1_upstream().await;
    let config = serde_yaml::from_str::<config::GatewayConfig>(&format!(
        r#"
spring:
  cloud:
    gateway:
      routes:
        - id: discovered
          uri: lb://localhost:{port}
          predicates:
            - Path=/discovered/**
"#,
        port = upstream.addr.port()
    ))
    .expect("scg lb config should parse");
    let gateway = spawn_gateway_with_routes(config.routes, temp_db_path()).await;

    let (status, body) = send_http1_request_with(
        gateway.addr,
        hyper::Method::GET,
        "gateway.local",
        "/discovered",
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, Bytes::from_static(b"http1"));

    gateway.finish().await;
    upstream.finish().await;
}

#[tokio::test]
async fn proxies_consul_backed_lb_route_definition() {
    let upstream = spawn_http1_upstream_with_body_for_connections(b"consul-discovered", 1).await;
    let instances = Arc::new(std::sync::Mutex::new(vec![RegistryInstance {
        host: "127.0.0.1".to_string(),
        port: upstream.addr.port(),
    }]));
    let registry = spawn_consul_registry("products", instances).await;
    let gateway_config = serde_yaml::from_str::<config::GatewayConfig>(&format!(
        r#"
server:
  discovery:
    refresh_ms: 50
    consul:
      address: http://127.0.0.1:{registry_port}
spring:
  cloud:
    gateway:
      routes:
        - id: products
          uri: lb://products
          predicates:
            - Path=/products/**
"#,
        registry_port = registry.addr.port()
    ))
    .expect("gateway config");
    let gateway = spawn_gateway_with_gateway_config(gateway_config, temp_db_path()).await;

    let (status, body) = send_http1_request_with(
        gateway.addr,
        hyper::Method::GET,
        "gateway.local",
        "/products",
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, Bytes::from_static(b"consul-discovered"));

    gateway.finish().await;
    upstream.finish().await;
    drop(registry);
}

#[tokio::test]
async fn refreshes_consul_backed_discovery_instances() {
    let upstream_a = spawn_http1_upstream_with_body_for_connections(b"consul-a", 1).await;
    let upstream_b = spawn_http1_upstream_with_body_for_connections(b"consul-b", 1).await;
    let instances = Arc::new(std::sync::Mutex::new(vec![RegistryInstance {
        host: "127.0.0.1".to_string(),
        port: upstream_a.addr.port(),
    }]));
    let registry = spawn_consul_registry("catalog", instances.clone()).await;
    let gateway_config = serde_yaml::from_str::<config::GatewayConfig>(&format!(
        r#"
server:
  discovery:
    refresh_ms: 50
    consul:
      address: http://127.0.0.1:{registry_port}
spring:
  cloud:
    gateway:
      routes:
        - id: catalog
          uri: lb://catalog
          predicates:
            - Path=/catalog/**
"#,
        registry_port = registry.addr.port()
    ))
    .expect("gateway config");
    let gateway = spawn_gateway_with_gateway_config(gateway_config, temp_db_path()).await;

    let (first_status, first_body) = send_http1_request_with(
        gateway.addr,
        hyper::Method::GET,
        "gateway.local",
        "/catalog",
    )
    .await;
    assert_eq!(first_status, StatusCode::OK);
    assert_eq!(first_body, Bytes::from_static(b"consul-a"));

    *instances.lock().unwrap() = vec![RegistryInstance {
        host: "127.0.0.1".to_string(),
        port: upstream_b.addr.port(),
    }];
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    let (second_status, second_body) = send_http1_request_with(
        gateway.addr,
        hyper::Method::GET,
        "gateway.local",
        "/catalog",
    )
    .await;
    assert_eq!(second_status, StatusCode::OK);
    assert_eq!(second_body, Bytes::from_static(b"consul-b"));

    gateway.finish().await;
    upstream_a.finish().await;
    upstream_b.finish().await;
    drop(registry);
}

#[tokio::test]
async fn proxies_eureka_backed_lb_route_definition() {
    let upstream = spawn_http1_upstream_with_body_for_connections(b"eureka-discovered", 1).await;
    let instances = Arc::new(std::sync::Mutex::new(vec![RegistryInstance {
        host: "127.0.0.1".to_string(),
        port: upstream.addr.port(),
    }]));
    let registry = spawn_eureka_registry("inventory", instances).await;
    let gateway_config = serde_yaml::from_str::<config::GatewayConfig>(&format!(
        r#"
server:
  discovery:
    refresh_ms: 50
    eureka:
      service_url: http://127.0.0.1:{registry_port}/eureka/v2/apps
      prefer_ip_address: true
spring:
  cloud:
    gateway:
      routes:
        - id: inventory
          uri: lb://inventory
          predicates:
            - Path=/inventory/**
"#,
        registry_port = registry.addr.port()
    ))
    .expect("gateway config");
    let gateway = spawn_gateway_with_gateway_config(gateway_config, temp_db_path()).await;

    let (status, body) = send_http1_request_with(
        gateway.addr,
        hyper::Method::GET,
        "gateway.local",
        "/inventory",
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, Bytes::from_static(b"eureka-discovered"));

    gateway.finish().await;
    upstream.finish().await;
    drop(registry);
}

#[tokio::test]
async fn serves_management_health_on_dedicated_listener_only() {
    let gateway = spawn_gateway_with_dedicated_management_listener(
        Arc::from([]),
        temp_db_path(),
        config::ManagementConfig {
            listen: Some("127.0.0.1:9000".to_string()),
            auth_token: None,
        },
    )
    .await;

    let (management_status, headers, body) = send_http1_request_without_api_key(
        gateway.management_addr,
        hyper::Method::GET,
        "gateway.local",
        "/__gateway/health",
    )
    .await;
    assert_eq!(management_status, StatusCode::OK);
    assert_eq!(
        headers
            .get(hyper::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/json")
    );
    assert_eq!(body, Bytes::from_static(br#"{"status":"UP"}"#));

    let (gateway_status, _, gateway_body) = send_http1_request_without_api_key(
        gateway.gateway_addr,
        hyper::Method::GET,
        "gateway.local",
        "/__gateway/health",
    )
    .await;
    assert_eq!(gateway_status, StatusCode::NOT_FOUND);
    assert!(gateway_body.is_empty());

    gateway.finish().await;
}

#[tokio::test]
async fn serves_management_routes_on_dedicated_listener() {
    let gateway = spawn_gateway_with_dedicated_management_listener(
        Arc::from([config::Route {
            scheme: "http".to_string(),
            protocol: config::Protocol::Http1,
            authority: config::Authority {
                host: "127.0.0.1".to_string(),
                port: "8001".to_string(),
            },
            discovery: None,
            upstreams: Arc::from([]),
            tls: config::RouteTlsConfig::default(),
            timeout_ms: 30_000,
            retry_attempts: 0,
            retry_statuses: Arc::from([]),
            retry_backoff_ms: 0,
            circuit_breaker: None,
            rate_limit: None,
            hosts: Arc::from(["api.example.com".to_string()]),
            methods: Arc::from(["GET".to_string()]),
            headers: BTreeMap::new(),
            queries: BTreeMap::new(),
            rewrite_prefix: None,
            request_header_filter: config::HeaderFilterConfig::default(),
            response_header_filter: config::HeaderFilterConfig::default(),
            path: "/api".to_string(),
        }]),
        temp_db_path(),
        config::ManagementConfig {
            listen: Some("127.0.0.1:9000".to_string()),
            auth_token: None,
        },
    )
    .await;

    let (status, headers, body) = send_http1_request_without_api_key(
        gateway.management_addr,
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
    assert!(body.contains(r#""path":"/api""#));
    assert!(body.contains(r#""hosts":["api.example.com"]"#));

    gateway.finish().await;
}

#[tokio::test]
async fn returns_not_found_for_non_management_path_on_dedicated_listener() {
    let gateway = spawn_gateway_with_dedicated_management_listener(
        Arc::from([]),
        temp_db_path(),
        config::ManagementConfig {
            listen: Some("127.0.0.1:9000".to_string()),
            auth_token: None,
        },
    )
    .await;

    let (status, _, body) = send_http1_request_without_api_key(
        gateway.management_addr,
        hyper::Method::GET,
        "gateway.local",
        "/",
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body.is_empty());

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
                discovery: None,
                upstreams: Arc::from([]),
                tls: config::RouteTlsConfig::default(),
                timeout_ms: 30_000,
                retry_attempts: 0,
                retry_statuses: Arc::from([]),
                retry_backoff_ms: 0,
                circuit_breaker: None,
                rate_limit: None,
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
                discovery: None,
                upstreams: Arc::from([]),
                tls: config::RouteTlsConfig::default(),
                timeout_ms: 5_000,
                retry_attempts: 1,
                retry_statuses: Arc::from([]),
                retry_backoff_ms: 0,
                circuit_breaker: Some(config::CircuitBreakerConfig {
                    failure_threshold: 2,
                    open_ms: 250,
                }),
                rate_limit: None,
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
async fn serves_management_metrics_snapshot_for_management_and_proxy_requests() {
    let upstream = spawn_http1_upstream().await;
    let gateway = spawn_gateway_once(
        config::Route {
            scheme: "http".to_string(),
            protocol: config::Protocol::Http1,
            authority: config::Authority {
                host: upstream.addr.ip().to_string(),
                port: upstream.addr.port().to_string(),
            },
            discovery: None,
            upstreams: Arc::from([]),
            tls: config::RouteTlsConfig::default(),
            timeout_ms: 30_000,
            retry_attempts: 0,
            retry_statuses: Arc::from([]),
            retry_backoff_ms: 0,
            circuit_breaker: None,
            rate_limit: None,
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

    let responses = send_http1_request_sequence(
        gateway.addr,
        &[
            Http1RequestSpec {
                method: hyper::Method::GET,
                host: "gateway.local",
                path: "/__gateway/health",
                include_api_key: false,
            },
            Http1RequestSpec {
                method: hyper::Method::GET,
                host: "gateway.local",
                path: "/",
                include_api_key: true,
            },
            Http1RequestSpec {
                method: hyper::Method::GET,
                host: "gateway.local",
                path: "/__gateway/metrics",
                include_api_key: false,
            },
        ],
    )
    .await;

    let (health_status, _, _) = &responses[0];
    assert_eq!(*health_status, StatusCode::OK);

    let (proxy_status, _, proxy_body) = &responses[1];
    assert_eq!(*proxy_status, StatusCode::OK);
    assert_eq!(*proxy_body, Bytes::from_static(b"http1"));

    let (metrics_status, headers, body) = &responses[2];
    assert_eq!(*metrics_status, StatusCode::OK);
    assert_eq!(
        headers
            .get(hyper::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/json")
    );

    let body: serde_json::Value = serde_json::from_slice(body).expect("metrics json");
    assert_eq!(body["requests_total"], 2);
    assert_eq!(body["errors_by_kind"], serde_json::json!({}));

    let routes = body["routes"].as_array().expect("routes array");
    let health_metrics = routes
        .iter()
        .find(|item| item["route"] == "/__gateway/health")
        .expect("health route metrics");
    assert_eq!(health_metrics["requests"], 1);
    assert_eq!(health_metrics["responses_by_status"]["200"], 1);

    let proxy_metrics = routes
        .iter()
        .find(|item| item["route"] == "/")
        .expect("proxy route metrics");
    assert_eq!(proxy_metrics["requests"], 1);
    assert_eq!(proxy_metrics["responses_by_status"]["200"], 1);

    let metrics_self = routes
        .iter()
        .find(|item| item["route"] == "/__gateway/metrics");
    assert!(metrics_self.is_none());

    gateway.finish().await;
    upstream.finish().await;
}

#[tokio::test]
async fn serves_management_metrics_snapshot_with_gateway_errors() {
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
            discovery: None,
            upstreams: Arc::from([]),
            tls: config::RouteTlsConfig::default(),
            timeout_ms: 30_000,
            retry_attempts: 0,
            retry_statuses: Arc::from([]),
            retry_backoff_ms: 0,
            circuit_breaker: None,
            rate_limit: None,
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

    let responses = send_http1_request_sequence(
        gateway.addr,
        &[
            Http1RequestSpec {
                method: hyper::Method::GET,
                host: "gateway.local",
                path: "/",
                include_api_key: true,
            },
            Http1RequestSpec {
                method: hyper::Method::GET,
                host: "gateway.local",
                path: "/__gateway/metrics",
                include_api_key: false,
            },
        ],
    )
    .await;

    let (status, _, body) = &responses[0];
    assert_eq!(*status, StatusCode::BAD_GATEWAY);
    assert!(body.is_empty());

    let (metrics_status, _, body) = &responses[1];
    assert_eq!(*metrics_status, StatusCode::OK);

    let body: serde_json::Value = serde_json::from_slice(body).expect("metrics json");
    assert_eq!(body["requests_total"], 1);
    assert_eq!(body["errors_by_kind"]["Io"], 1);

    let routes = body["routes"].as_array().expect("routes array");
    let proxy_metrics = routes
        .iter()
        .find(|item| item["route"] == "/")
        .expect("proxy route metrics");
    assert_eq!(proxy_metrics["requests"], 1);
    assert_eq!(proxy_metrics["responses_by_status"]["502"], 1);

    gateway.finish().await;
}

#[tokio::test]
async fn rejects_management_requests_without_configured_bearer_token() {
    let gateway = spawn_gateway_with_management_config(
        Arc::from([]),
        temp_db_path(),
        config::ManagementConfig {
            listen: None,
            auth_token: Some("secret-token".to_string()),
        },
    )
    .await;

    let (status, headers, body) = send_http1_request_without_api_key(
        gateway.addr,
        hyper::Method::GET,
        "gateway.local",
        "/__gateway/health",
    )
    .await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        headers
            .get(hyper::header::WWW_AUTHENTICATE)
            .and_then(|value| value.to_str().ok()),
        Some("Bearer")
    );
    assert!(body.is_empty());

    gateway.finish().await;
}

#[tokio::test]
async fn serves_management_requests_with_configured_bearer_token() {
    let gateway = spawn_gateway_with_management_config(
        Arc::from([]),
        temp_db_path(),
        config::ManagementConfig {
            listen: None,
            auth_token: Some("secret-token".to_string()),
        },
    )
    .await;

    let (status, headers, body) = send_http1_request_without_api_key_with_headers(
        gateway.addr,
        hyper::Method::GET,
        "gateway.local",
        "/__gateway/health",
        &[("authorization", "Bearer secret-token")],
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
            discovery: None,
            upstreams: Arc::from([config::Authority {
                host: upstream_b.addr.ip().to_string(),
                port: upstream_b.addr.port().to_string(),
            }]),
            tls: config::RouteTlsConfig::default(),
            timeout_ms: 30_000,
            retry_attempts: 0,
            retry_statuses: Arc::from([]),
            retry_backoff_ms: 0,
            circuit_breaker: None,
            rate_limit: None,
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
            discovery: None,
            upstreams: Arc::from([config::Authority {
                host: backup.addr.ip().to_string(),
                port: backup.addr.port().to_string(),
            }]),
            tls: config::RouteTlsConfig::default(),
            timeout_ms: 30_000,
            retry_attempts: 1,
            retry_statuses: Arc::from([]),
            retry_backoff_ms: 0,
            circuit_breaker: None,
            rate_limit: None,
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
async fn retries_retryable_upstream_status_on_next_backend() {
    let primary =
        spawn_http1_upstream_with_status_sequence(vec![StatusCode::SERVICE_UNAVAILABLE]).await;
    let backup =
        spawn_http1_upstream_with_body_for_connections(b"retryable-status-backup", 1).await;
    let gateway = spawn_gateway_once(
        config::Route {
            scheme: "http".to_string(),
            protocol: config::Protocol::Http1,
            authority: config::Authority {
                host: primary.addr.ip().to_string(),
                port: primary.addr.port().to_string(),
            },
            discovery: None,
            upstreams: Arc::from([config::Authority {
                host: backup.addr.ip().to_string(),
                port: backup.addr.port().to_string(),
            }]),
            tls: config::RouteTlsConfig::default(),
            timeout_ms: 30_000,
            retry_attempts: 1,
            retry_statuses: Arc::from([StatusCode::SERVICE_UNAVAILABLE.as_u16()]),
            retry_backoff_ms: 0,
            circuit_breaker: None,
            rate_limit: None,
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
    assert_eq!(body, Bytes::from_static(b"retryable-status-backup"));

    gateway.finish().await;
    primary.finish().await;
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
            discovery: None,
            upstreams: Arc::from([config::Authority {
                host: backup.addr.ip().to_string(),
                port: backup.addr.port().to_string(),
            }]),
            tls: config::RouteTlsConfig::default(),
            timeout_ms: 10,
            retry_attempts: 1,
            retry_statuses: Arc::from([]),
            retry_backoff_ms: 0,
            circuit_breaker: None,
            rate_limit: None,
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
            discovery: None,
            upstreams: Arc::from([]),
            tls: config::RouteTlsConfig::default(),
            timeout_ms: 30_000,
            retry_attempts: 0,
            retry_statuses: Arc::from([]),
            retry_backoff_ms: 0,
            circuit_breaker: None,
            rate_limit: None,
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
            discovery: None,
            upstreams: Arc::from([]),
            tls: config::RouteTlsConfig::default(),
            timeout_ms: 30_000,
            retry_attempts: 0,
            retry_statuses: Arc::from([]),
            retry_backoff_ms: 0,
            circuit_breaker: None,
            rate_limit: None,
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
            discovery: None,
            upstreams: Arc::from([]),
            tls: config::RouteTlsConfig::default(),
            timeout_ms: 30_000,
            retry_attempts: 0,
            retry_statuses: Arc::from([]),
            retry_backoff_ms: 0,
            circuit_breaker: None,
            rate_limit: None,
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
async fn proxies_common_request_bodies_without_content_type_rewrite() {
    let upstream = spawn_http1_upstream_echo_content_type_and_body(3).await;
    let gateway = spawn_gateway_once(
        config::Route {
            scheme: "http".to_string(),
            protocol: config::Protocol::Http1,
            authority: config::Authority {
                host: upstream.addr.ip().to_string(),
                port: upstream.addr.port().to_string(),
            },
            discovery: None,
            upstreams: Arc::from([]),
            tls: config::RouteTlsConfig::default(),
            timeout_ms: 30_000,
            retry_attempts: 0,
            retry_statuses: Arc::from([]),
            retry_backoff_ms: 0,
            circuit_breaker: None,
            rate_limit: None,
            hosts: Arc::from([]),
            methods: Arc::from([]),
            headers: BTreeMap::new(),
            queries: BTreeMap::new(),
            rewrite_prefix: None,
            request_header_filter: config::HeaderFilterConfig::default(),
            response_header_filter: config::HeaderFilterConfig::default(),
            path: "/submit".to_string(),
        },
        temp_db_path(),
    )
    .await;

    let cases = [
        (
            "application/json",
            r#"{"name":"alice","role":"admin"}"#,
        ),
        (
            "application/x-www-form-urlencoded",
            "name=alice&role=admin",
        ),
        (
            "multipart/form-data; boundary=boundary123",
            "--boundary123\r\nContent-Disposition: form-data; name=\"name\"\r\n\r\nalice\r\n--boundary123\r\nContent-Disposition: form-data; name=\"file\"; filename=\"hello.txt\"\r\nContent-Type: text/plain\r\n\r\nhello world\r\n--boundary123--\r\n",
        ),
    ];

    for (content_type, body) in cases {
        let (status, echoed) = send_http1_request_with_body(
            gateway.addr,
            hyper::Method::POST,
            "gateway.local",
            "/submit",
            &[("content-type", content_type)],
            Bytes::from(body.to_string()),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            echoed,
            Bytes::from(format!("{content_type}\n{body}")),
            "gateway should forward request body and content-type unchanged"
        );
    }

    gateway.finish().await;
    upstream.finish().await;
}

#[tokio::test]
async fn streams_request_body_to_upstream_when_retries_are_disabled() {
    let (upstream, first_chunk_rx) = spawn_http1_upstream_signal_first_body_chunk().await;
    let gateway = spawn_gateway_once(
        config::Route {
            scheme: "http".to_string(),
            protocol: config::Protocol::Http1,
            authority: config::Authority {
                host: upstream.addr.ip().to_string(),
                port: upstream.addr.port().to_string(),
            },
            discovery: None,
            upstreams: Arc::from([]),
            tls: config::RouteTlsConfig::default(),
            timeout_ms: 30_000,
            retry_attempts: 0,
            retry_statuses: Arc::from([]),
            retry_backoff_ms: 0,
            circuit_breaker: None,
            rate_limit: None,
            hosts: Arc::from([]),
            methods: Arc::from([]),
            headers: BTreeMap::new(),
            queries: BTreeMap::new(),
            rewrite_prefix: None,
            request_header_filter: config::HeaderFilterConfig::default(),
            response_header_filter: config::HeaderFilterConfig::default(),
            path: "/upload".to_string(),
        },
        temp_db_path(),
    )
    .await;

    let mut client = tokio::net::TcpStream::connect(gateway.addr)
        .await
        .expect("connect gateway");
    client
        .write_all(
            concat!(
                "POST /upload HTTP/1.1\r\n",
                "Host: gateway.local\r\n",
                "api-key: 1\r\n",
                "Content-Type: application/octet-stream\r\n",
                "Transfer-Encoding: chunked\r\n",
                "Connection: close\r\n",
                "\r\n",
            )
            .as_bytes(),
        )
        .await
        .expect("write request headers");
    client
        .write_all(b"5\r\nhello\r\n")
        .await
        .expect("write first body chunk");
    client.flush().await.expect("flush first chunk");

    let first_chunk = tokio::time::timeout(std::time::Duration::from_millis(200), first_chunk_rx)
        .await
        .expect("gateway should stream the first chunk before the request completes")
        .expect("upstream should receive a first chunk");
    assert_eq!(first_chunk, Bytes::from_static(b"hello"));

    client
        .write_all(b"6\r\n world\r\n0\r\n\r\n")
        .await
        .expect("write remaining body");

    let mut response = Vec::new();
    tokio::time::timeout(
        std::time::Duration::from_millis(500),
        client.read_to_end(&mut response),
    )
    .await
    .expect("gateway should return a response")
    .expect("read response");
    let response = String::from_utf8_lossy(&response);
    assert!(response.contains("200 OK"));
    assert!(response.contains("streamed"));

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
                discovery: None,
                upstreams: Arc::from([]),
                tls: config::RouteTlsConfig::default(),
                timeout_ms: 30_000,
                retry_attempts: 0,
                retry_statuses: Arc::from([]),
                retry_backoff_ms: 0,
                circuit_breaker: None,
                rate_limit: None,
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
                discovery: None,
                upstreams: Arc::from([]),
                tls: config::RouteTlsConfig::default(),
                timeout_ms: 30_000,
                retry_attempts: 0,
                retry_statuses: Arc::from([]),
                retry_backoff_ms: 0,
                circuit_breaker: None,
                rate_limit: None,
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
                discovery: None,
                upstreams: Arc::from([]),
                tls: config::RouteTlsConfig::default(),
                timeout_ms: 30_000,
                retry_attempts: 0,
                retry_statuses: Arc::from([]),
                retry_backoff_ms: 0,
                circuit_breaker: None,
                rate_limit: None,
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
                discovery: None,
                upstreams: Arc::from([]),
                tls: config::RouteTlsConfig::default(),
                timeout_ms: 30_000,
                retry_attempts: 0,
                retry_statuses: Arc::from([]),
                retry_backoff_ms: 0,
                circuit_breaker: None,
                rate_limit: None,
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
                discovery: None,
                upstreams: Arc::from([]),
                tls: config::RouteTlsConfig::default(),
                timeout_ms: 30_000,
                retry_attempts: 0,
                retry_statuses: Arc::from([]),
                retry_backoff_ms: 0,
                circuit_breaker: None,
                rate_limit: None,
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
                discovery: None,
                upstreams: Arc::from([]),
                tls: config::RouteTlsConfig::default(),
                timeout_ms: 30_000,
                retry_attempts: 0,
                retry_statuses: Arc::from([]),
                retry_backoff_ms: 0,
                circuit_breaker: None,
                rate_limit: None,
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
                discovery: None,
                upstreams: Arc::from([]),
                tls: config::RouteTlsConfig::default(),
                timeout_ms: 30_000,
                retry_attempts: 0,
                retry_statuses: Arc::from([]),
                retry_backoff_ms: 0,
                circuit_breaker: None,
                rate_limit: None,
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
                discovery: None,
                upstreams: Arc::from([]),
                tls: config::RouteTlsConfig::default(),
                timeout_ms: 30_000,
                retry_attempts: 0,
                retry_statuses: Arc::from([]),
                retry_backoff_ms: 0,
                circuit_breaker: None,
                rate_limit: None,
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
            discovery: None,
            upstreams: Arc::from([]),
            tls: config::RouteTlsConfig::default(),
            timeout_ms: 30_000,
            retry_attempts: 0,
            retry_statuses: Arc::from([]),
            retry_backoff_ms: 0,
            circuit_breaker: None,
            rate_limit: None,
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
            discovery: None,
            upstreams: Arc::from([]),
            tls: config::RouteTlsConfig {
                ca_cert_path: Some(tls_fixture.cert_path.display().to_string()),
            },
            timeout_ms: 30_000,
            retry_attempts: 0,
            retry_statuses: Arc::from([]),
            retry_backoff_ms: 0,
            circuit_breaker: None,
            rate_limit: None,
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
            discovery: None,
            upstreams: Arc::from([]),
            tls: config::RouteTlsConfig::default(),
            timeout_ms: 30_000,
            retry_attempts: 0,
            retry_statuses: Arc::from([]),
            retry_backoff_ms: 0,
            circuit_breaker: None,
            rate_limit: None,
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
            discovery: None,
            upstreams: Arc::from([]),
            tls: config::RouteTlsConfig::default(),
            timeout_ms: 10,
            retry_attempts: 0,
            retry_statuses: Arc::from([]),
            retry_backoff_ms: 0,
            circuit_breaker: None,
            rate_limit: None,
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
            discovery: None,
            upstreams: Arc::from([]),
            tls: config::RouteTlsConfig::default(),
            timeout_ms: 30_000,
            retry_attempts: 0,
            retry_statuses: Arc::from([]),
            retry_backoff_ms: 0,
            circuit_breaker: Some(config::CircuitBreakerConfig {
                failure_threshold: 1,
                open_ms: 250,
            }),
            rate_limit: None,
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
            discovery: None,
            upstreams: Arc::from([]),
            tls: config::RouteTlsConfig::default(),
            timeout_ms: 30_000,
            retry_attempts: 0,
            retry_statuses: Arc::from([]),
            retry_backoff_ms: 0,
            circuit_breaker: None,
            rate_limit: None,
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
            discovery: None,
            upstreams: Arc::from([]),
            tls: config::RouteTlsConfig {
                ca_cert_path: Some(tls_fixture.cert_path.display().to_string()),
            },
            timeout_ms: 30_000,
            retry_attempts: 0,
            retry_statuses: Arc::from([]),
            retry_backoff_ms: 0,
            circuit_breaker: None,
            rate_limit: None,
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
            discovery: None,
            upstreams: Arc::from([]),
            tls: config::RouteTlsConfig::default(),
            timeout_ms: 30_000,
            retry_attempts: 0,
            retry_statuses: Arc::from([]),
            retry_backoff_ms: 0,
            circuit_breaker: None,
            rate_limit: None,
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
            discovery: None,
            upstreams: Arc::from([]),
            tls: config::RouteTlsConfig::default(),
            timeout_ms: 30_000,
            retry_attempts: 0,
            retry_statuses: Arc::from([]),
            retry_backoff_ms: 0,
            circuit_breaker: None,
            rate_limit: None,
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
            discovery: None,
            upstreams: Arc::from([]),
            tls: config::RouteTlsConfig::default(),
            timeout_ms: 30_000,
            retry_attempts: 0,
            retry_statuses: Arc::from([]),
            retry_backoff_ms: 0,
            circuit_breaker: None,
            rate_limit: None,
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
            discovery: None,
            upstreams: Arc::from([]),
            tls: config::RouteTlsConfig::default(),
            timeout_ms: 30_000,
            retry_attempts: 0,
            retry_statuses: Arc::from([]),
            retry_backoff_ms: 0,
            circuit_breaker: None,
            rate_limit: None,
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
                discovery: None,
                upstreams: Arc::from([]),
                tls: config::RouteTlsConfig::default(),
                timeout_ms: 30_000,
                retry_attempts: 0,
                retry_statuses: Arc::from([]),
                retry_backoff_ms: 0,
                circuit_breaker: None,
                rate_limit: None,
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
                discovery: None,
                upstreams: Arc::from([]),
                tls: config::RouteTlsConfig::default(),
                timeout_ms: 30_000,
                retry_attempts: 0,
                retry_statuses: Arc::from([]),
                retry_backoff_ms: 0,
                circuit_breaker: None,
                rate_limit: None,
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
            discovery: None,
            upstreams: Arc::from([]),
            tls: config::RouteTlsConfig::default(),
            timeout_ms: 30_000,
            retry_attempts: 0,
            retry_statuses: Arc::from([]),
            retry_backoff_ms: 0,
            circuit_breaker: None,
            rate_limit: None,
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
