use http_body_util::{BodyExt, Empty, Full};
use hyper::{body::Bytes, Request, Response, StatusCode};
use hyper_gateway::{config, config::Protocol, entity, logger, route, throttle, trace};
use hyper_util::rt::{TokioExecutor, TokioIo};
use std::{path::PathBuf, sync::Arc};

fn temp_db_path() -> PathBuf {
    std::env::temp_dir().join(format!("gateway-throttle-{}.db", fastrand::u64(..)))
}

#[test]
fn gateway_library_exports_public_modules() {
    assert_eq!(format!("{:?}", Protocol::Http1), "Http1");
    let _ = entity::get_gateway_response;
    let _ = logger::init_logger as fn();
    let _ = route::run;
    let _ = throttle::init_throttle;
    let _ = trace::new;

    let db_path = temp_db_path();
    let pool =
        throttle::init_throttle_at(&db_path).expect("test-friendly throttle API should work");
    drop(pool);
    std::fs::remove_file(&db_path).expect("temporary throttle db should be removed");
}

async fn spawn_gateway_once(route_config: config::Route, db_path: PathBuf) -> std::net::SocketAddr {
    let throttle_pool = throttle::init_throttle_at(&db_path).expect("throttle pool");
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    let routes: Arc<[config::Route]> = Arc::from([route_config]);

    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        route::run(stream, routes, throttle_pool).await.unwrap();
    });

    addr
}

async fn spawn_http1_upstream() -> std::net::SocketAddr {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
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

    addr
}

async fn spawn_h2c_upstream() -> std::net::SocketAddr {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
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

    addr
}

async fn send_http1_request(addr: std::net::SocketAddr) -> (StatusCode, Bytes) {
    let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let (mut sender, conn) = hyper::client::conn::http1::Builder::new()
        .handshake::<_, Empty<Bytes>>(TokioIo::new(stream))
        .await
        .unwrap();

    tokio::spawn(async move {
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
    (status, body)
}

async fn send_h2c_request(addr: std::net::SocketAddr) -> (StatusCode, Bytes) {
    let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let (mut sender, conn) = hyper::client::conn::http2::Builder::new(TokioExecutor::new())
        .handshake::<_, Empty<Bytes>>(TokioIo::new(stream))
        .await
        .unwrap();

    tokio::spawn(async move {
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
                host: upstream.ip().to_string(),
                port: upstream.port().to_string(),
            },
            path: "/".to_string(),
        },
        temp_db_path(),
    )
    .await;

    let (status, body) = send_http1_request(gateway).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, Bytes::from_static(b"http1"));
}

#[tokio::test]
async fn supports_h2c_gateway_to_h2c_upstream() {
    let upstream = spawn_h2c_upstream().await;
    let gateway = spawn_gateway_once(
        config::Route {
            scheme: "http".to_string(),
            protocol: config::Protocol::H2c,
            authority: config::Authority {
                host: upstream.ip().to_string(),
                port: upstream.port().to_string(),
            },
            path: "/".to_string(),
        },
        temp_db_path(),
    )
    .await;

    let (status, body) = send_h2c_request(gateway).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, Bytes::from_static(b"h2c"));
}

#[tokio::test]
async fn supports_h2c_client_to_http1_upstream() {
    let upstream = spawn_http1_upstream().await;
    let gateway = spawn_gateway_once(
        config::Route {
            scheme: "http".to_string(),
            protocol: config::Protocol::Http1,
            authority: config::Authority {
                host: upstream.ip().to_string(),
                port: upstream.port().to_string(),
            },
            path: "/".to_string(),
        },
        temp_db_path(),
    )
    .await;

    let (status, body) = send_h2c_request(gateway).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, Bytes::from_static(b"http1"));
}
