use hyper_gateway::{config, logger, route, throttle, tls};
use std::sync::{Arc, Mutex};

#[tokio::main]
async fn main() {
    logger::init_logger();
    log::info!("Initialized the logger");

    let gateway_config: config::GatewayConfig = config::load_config("config.yaml");
    let gateway_addr = gateway_config.server.listen.clone();
    let gateway_tls = gateway_config.server.tls.clone();
    let route_config_arr: Arc<[config::Route]> = gateway_config.routes;

    let throttle_pool: Arc<Mutex<r2d2::Pool<r2d2_sqlite::SqliteConnectionManager>>> =
        throttle::init_throttle().expect("Failed to initialize the throttle!");

    let gateway_listener = tokio::net::TcpListener::bind(gateway_addr.as_str())
        .await
        .expect("Failed to create the TCP listener!");
    let bound_addr = gateway_listener
        .local_addr()
        .expect("Failed to resolve the listener address!");
    let tls_acceptor = gateway_tls.as_ref().map(|server_tls| {
        tokio_rustls::TlsAcceptor::from(Arc::new(
            tls::build_server_config(server_tls)
                .expect("Failed to build the gateway TLS server config!"),
        ))
    });

    let scheme = if tls_acceptor.is_some() {
        "https"
    } else {
        "http"
    };
    log::info!("Listening on {}://{}", scheme, &bound_addr);
    loop {
        let (gateway_stream, _) = gateway_listener
            .accept()
            .await
            .expect("Failed to accept a connection from this listener!");
        let route_config_arr = route_config_arr.clone();
        let throttle_pool = throttle_pool.clone();
        let tls_acceptor = tls_acceptor.clone();

        tokio::spawn(async move {
            let result = match tls_acceptor {
                Some(tls_acceptor) => match tls_acceptor.accept(gateway_stream).await {
                    Ok(stream) => {
                        route::serve_connection(stream, route_config_arr, throttle_pool).await
                    }
                    Err(err) => {
                        log::error!("Gateway TLS handshake failed: {:?}", err);
                        return;
                    }
                },
                None => route::run(gateway_stream, route_config_arr, throttle_pool).await,
            };

            if let Err(err) = result {
                log::error!("Gateway connection failed: {:?}", err);
            }
        });
    }
}
