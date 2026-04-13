use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use ed25519_dalek::{Signature, Verifier};

const TEST_SIGNING_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEIKsGy9B4I0tmInArgmTzxXb2Y3ahKe19Alib2Gj97X9S\n-----END PRIVATE KEY-----\n";

fn write_signing_key_pem() -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be valid")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("auth-server-token-signing-key-{unique}.pem"));
    fs::write(&path, TEST_SIGNING_KEY_PEM).expect("test signing key should be written");
    path
}

#[test]
fn token_service_issues_eddsa_access_tokens() {
    let signing_key_path = write_signing_key_pem();
    let signing_keys =
        auth_server::token::SigningKeys::from_pem_file(&signing_key_path).expect("key should load");
    let service = auth_server::token::TokenService::new("https://issuer.example", signing_keys);

    let token = service
        .issue_access_token(auth_server::token::AccessTokenParams {
            subject: "user-123",
            audience: "api://default",
            scope: "openid profile",
            roles: Some(vec!["admin".to_owned()]),
            client_id: Some("test-client"),
            authorized_party: Some("test-client"),
            ttl_seconds: 300,
        })
        .expect("token should issue");

    let segments: Vec<&str> = token.split('.').collect();
    assert_eq!(segments.len(), 3);

    let header: serde_json::Value = serde_json::from_slice(
        &URL_SAFE_NO_PAD
            .decode(segments[0])
            .expect("header should be valid base64url"),
    )
    .expect("header should be valid json");
    assert_eq!(header["alg"], "EdDSA");
    assert_eq!(header["typ"], "JWT");

    let claims: serde_json::Value = serde_json::from_slice(
        &URL_SAFE_NO_PAD
            .decode(segments[1])
            .expect("claims should be valid base64url"),
    )
    .expect("claims should be valid json");
    assert_eq!(claims["iss"], "https://issuer.example");
    assert_eq!(claims["sub"], "user-123");
    assert_eq!(claims["aud"], "api://default");
    assert_eq!(claims["scope"], "openid profile");
    assert_eq!(claims["client_id"], "test-client");
    assert_eq!(claims["azp"], "test-client");
    assert_eq!(claims["roles"], serde_json::json!(["admin"]));
    assert!(claims["iat"].as_i64().is_some());
    assert!(claims["exp"].as_i64().is_some());
    assert!(claims["jti"].as_str().is_some());

    let jwks = service.jwks();
    assert_eq!(jwks.keys.len(), 1);
    assert_eq!(header["kid"], jwks.keys[0].kid);

    let public_key_bytes_vec = URL_SAFE_NO_PAD
        .decode(&jwks.keys[0].x)
        .expect("jwk x should be valid base64url");
    let public_key_bytes: [u8; 32] = public_key_bytes_vec
        .try_into()
        .expect("Ed25519 public key should be 32 bytes");
    let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(&public_key_bytes)
        .expect("public key should parse");
    let signature = Signature::from_slice(
        &URL_SAFE_NO_PAD
            .decode(segments[2])
            .expect("signature should be valid base64url"),
    )
    .expect("signature should parse");
    verifying_key
        .verify(
            format!("{}.{}", segments[0], segments[1]).as_bytes(),
            &signature,
        )
        .expect("signature should verify");

    fs::remove_file(signing_key_path).expect("temp key should be removed");
}
