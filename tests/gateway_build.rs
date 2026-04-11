use hyper_gateway::config::Protocol;

#[test]
fn gateway_library_exports_protocol() {
    assert_eq!(format!("{:?}", Protocol::Http1), "Http1");
}
