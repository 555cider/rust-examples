use http_body_util::{BodyExt, Empty, Full};
use hyper::{body::Bytes, Request, Response, StatusCode};
use hyper_gateway::{config, config::Protocol, entity, logger, route, throttle, trace};
use hyper_util::rt::{TokioExecutor, TokioIo};
use std::{
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
            .unwrap();
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

async fn send_http1_request(addr: std::net::SocketAddr) -> (StatusCode, Bytes) {
    let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let (mut sender, conn) = hyper::client::conn::http1::Builder::new()
        .handshake::<_, Empty<Bytes>>(TokioIo::new(stream))
        .await
        .unwrap();

    let conn_task = tokio::spawn(async move {
        conn.await.unwrap();
    });

    let request = Request::builder()
        .uri("http://gateway.local/")
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
                protocol: config::Protocol::H2c,
                authority: config::Authority {
                    host: h2c_upstream.addr.ip().to_string(),
                    port: h2c_upstream.addr.port().to_string(),
                },
                path: "/h2".to_string(),
            },
            config::Route {
                scheme: "http".to_string(),
                protocol: config::Protocol::Http1,
                authority: config::Authority {
                    host: default_upstream.addr.ip().to_string(),
                    port: default_upstream.addr.port().to_string(),
                },
                path: "/".to_string(),
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
