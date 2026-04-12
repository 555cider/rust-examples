use hyper_gateway::{config, logger, metrics, route, throttle, tls};
use std::sync::Arc;

#[tokio::main]
async fn main() {
    logger::init_logger();
    log::info!("Initialized the logger");

    let gateway_config: config::GatewayConfig = config::load_config("config.yaml");
    let gateway_addr = gateway_config.server.listen.clone();
    let gateway_tls = gateway_config.server.tls.clone();
    let management_config = gateway_config.server.management.clone();
    let dedicated_management_addr = management_config.listen.clone();
    let route_config_arr: Arc<[config::Route]> = gateway_config.routes;

    let throttle_pool: throttle::ThrottlePool =
        throttle::init_throttle().expect("Failed to initialize the throttle!");
    let metrics_collector = metrics::new_collector();

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
    if let Some(management_addr) = dedicated_management_addr.clone() {
        let management_listener = tokio::net::TcpListener::bind(management_addr.as_str())
            .await
            .expect("Failed to create the management TCP listener!");
        let management_bound_addr = management_listener
            .local_addr()
            .expect("Failed to resolve the management listener address!");
        log::info!("Management listening on http://{}", &management_bound_addr);

        let management_routes = route_config_arr.clone();
        let management_metrics = metrics_collector.clone();
        let management_config = management_config.clone();
        tokio::spawn(async move {
            loop {
                let (management_stream, _) = management_listener
                    .accept()
                    .await
                    .expect("Failed to accept a connection from the management listener!");
                let management_routes = management_routes.clone();
                let management_metrics = management_metrics.clone();
                let management_config = management_config.clone();

                tokio::spawn(async move {
                    if let Err(err) = hyper_gateway::management::run(
                        management_stream,
                        management_routes,
                        management_metrics,
                        management_config,
                    )
                    .await
                    {
                        log::error!("Management connection failed: {:?}", err);
                    }
                });
            }
        });
    }

    let dedicated_management_enabled = dedicated_management_addr.is_some();
    loop {
        let (gateway_stream, _) = gateway_listener
            .accept()
            .await
            .expect("Failed to accept a connection from this listener!");
        let route_config_arr = route_config_arr.clone();
        let throttle_pool = throttle_pool.clone();
        let metrics_collector = metrics_collector.clone();
        let management_config = management_config.clone();
        let tls_acceptor = tls_acceptor.clone();

        tokio::spawn(async move {
            let route_config_arr_for_conn = route_config_arr.clone();
            let throttle_pool_for_conn = throttle_pool.clone();
            let metrics_for_conn = metrics_collector.clone();
            let management_for_conn = management_config.clone();

            let result = match tls_acceptor {
                Some(tls_acceptor) => match tls_acceptor.accept(gateway_stream).await {
                    Ok(stream) => {
                        if dedicated_management_enabled {
                            route::run_proxy_only(
                                stream,
                                route_config_arr_for_conn,
                                throttle_pool_for_conn,
                                metrics_for_conn,
                            )
                            .await
                        } else {
                            route::serve_connection(
                                stream,
                                route_config_arr_for_conn,
                                throttle_pool_for_conn,
                                metrics_for_conn,
                                management_for_conn,
                            )
                            .await
                        }
                    }
                    Err(err) => {
                        log::error!("Gateway TLS handshake failed: {:?}", err);
                        return;
                    }
                },
                None => {
                    if dedicated_management_enabled {
                        route::run_proxy_only(
                            gateway_stream,
                            route_config_arr_for_conn,
                            throttle_pool_for_conn,
                            metrics_for_conn,
                        )
                        .await
                    } else {
                        route::run(
                            gateway_stream,
                            route_config_arr_for_conn,
                            throttle_pool_for_conn,
                            metrics_for_conn,
                            management_for_conn,
                        )
                        .await
                    }
                }
            };

            if let Err(err) = result {
                log::error!("Gateway connection failed: {:?}", err);
            }
        });
    }
}
