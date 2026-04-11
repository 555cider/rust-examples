use hyper_gateway::{config::Protocol, entity, logger, route, throttle, trace};

#[test]
fn gateway_library_exports_public_modules() {
    assert_eq!(format!("{:?}", Protocol::Http1), "Http1");
    let _ = entity::get_gateway_response;
    let _ = logger::init_logger as fn();
    let _ = route::run;
    let _ = throttle::init_throttle;
    let _ = trace::new;

    let db_path = std::env::temp_dir().join(format!("gateway-build-{}.db", fastrand::u64(..)));
    let pool = throttle::init_throttle_at(&db_path).expect("test-friendly throttle API should work");
    drop(pool);
    std::fs::remove_file(&db_path).expect("temporary throttle db should be removed");
}
