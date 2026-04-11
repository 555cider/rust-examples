use hyper_gateway::{config, logger, route, throttle};
use std::{
    sync::{Arc, Mutex},
};

#[tokio::main]
async fn main() {
    logger::init_logger();
    log::info!("Initialized the logger");

    let gateway_config: config::GatewayConfig = config::load_config("config.yaml");
    let gateway_addr = gateway_config.server.listen.clone();
    let route_config_arr: Arc<[config::Route]> = gateway_config.routes;

    let throttle_pool: Arc<Mutex<r2d2::Pool<r2d2_sqlite::SqliteConnectionManager>>> =
        throttle::init_throttle().expect("Failed to initialize the throttle!");

    let gateway_listener = tokio::net::TcpListener::bind(gateway_addr.as_str())
        .await
        .expect("Failed to create the TCP listener!");
    let bound_addr = gateway_listener
        .local_addr()
        .expect("Failed to resolve the listener address!");

    log::info!("Listening on http://{}", &bound_addr);
    loop {
        let (gateway_stream, _) = gateway_listener
            .accept()
            .await
            .expect("Failed to accept a connection from this listener!");
        let route_config_arr = route_config_arr.clone();
        let throttle_pool = throttle_pool.clone();

        tokio::spawn(async move {
            if let Err(err) = route::run(gateway_stream, route_config_arr, throttle_pool).await {
                log::error!("Gateway connection failed: {:?}", err);
            }
        });
    }
}
