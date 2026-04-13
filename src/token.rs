use std::{
    fs,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use ed25519_dalek::{Signer, SigningKey, pkcs8::DecodePrivateKey};
use rand::rngs::OsRng;
use serde::Serialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::error::{AppError, Result};

#[derive(Debug, Clone, Serialize)]
pub struct JwkSet {
    pub keys: Vec<Jwk>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Jwk {
    pub kty: &'static str,
    #[serde(rename = "use")]
    pub key_use: &'static str,
    pub crv: &'static str,
    pub alg: &'static str,
    pub kid: String,
    pub x: String,
}

#[derive(Clone)]
pub struct SigningKeys {
    signing_key: SigningKey,
    public_jwk: Jwk,
}

impl SigningKeys {
    pub fn from_pem_file(path: impl AsRef<Path>) -> Result<Self> {
        let pem = fs::read_to_string(path)?;
        let signing_key =
            SigningKey::from_pkcs8_pem(&pem).map_err(|error| AppError::Token(error.to_string()))?;
        Ok(Self::from_signing_key(signing_key))
    }

    pub fn generate_ephemeral() -> Self {
        let signing_key = SigningKey::generate(&mut OsRng);
        Self::from_signing_key(signing_key)
    }

    pub fn jwk(&self) -> Jwk {
        self.public_jwk.clone()
    }

    fn from_signing_key(signing_key: SigningKey) -> Self {
        let public_key_bytes = signing_key.verifying_key().to_bytes();
        let x = URL_SAFE_NO_PAD.encode(public_key_bytes);
        let kid = URL_SAFE_NO_PAD.encode(Sha256::digest(public_key_bytes));

        Self {
            signing_key,
            public_jwk: Jwk {
                kty: "OKP",
                key_use: "sig",
                crv: "Ed25519",
                alg: "EdDSA",
                kid,
                x,
            },
        }
    }
}

#[derive(Clone)]
pub struct TokenService {
    issuer: String,
    signing_keys: SigningKeys,
}

impl TokenService {
    pub fn new(issuer: impl Into<String>, signing_keys: SigningKeys) -> Self {
        Self {
            issuer: issuer.into().trim_end_matches('/').to_owned(),
            signing_keys,
        }
    }

    pub fn jwks(&self) -> JwkSet {
        JwkSet {
            keys: vec![self.signing_keys.jwk()],
        }
    }

    pub fn issue_access_token(&self, params: AccessTokenParams<'_>) -> Result<String> {
        let issued_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| AppError::Token(error.to_string()))?
            .as_secs() as i64;
        let expires_at = issued_at + params.ttl_seconds as i64;

        let header = JwtHeader {
            alg: "EdDSA",
            typ: "JWT",
            kid: self.signing_keys.public_jwk.kid.clone(),
        };
        let claims = AccessTokenClaims {
            iss: self.issuer.clone(),
            sub: params.subject.to_owned(),
            aud: params.audience.to_owned(),
            exp: expires_at,
            iat: issued_at,
            jti: Uuid::new_v4().to_string(),
            scope: params.scope.to_owned(),
            roles: params.roles,
            client_id: params.client_id.map(str::to_owned),
            azp: params.authorized_party.map(str::to_owned),
        };

        let encoded_header = encode_json(&header)?;
        let encoded_claims = encode_json(&claims)?;
        let signing_input = format!("{encoded_header}.{encoded_claims}");
        let signature = self.signing_keys.signing_key.sign(signing_input.as_bytes());
        let encoded_signature = URL_SAFE_NO_PAD.encode(signature.to_bytes());

        Ok(format!("{signing_input}.{encoded_signature}"))
    }
}

pub struct AccessTokenParams<'a> {
    pub subject: &'a str,
    pub audience: &'a str,
    pub scope: &'a str,
    pub roles: Option<Vec<String>>,
    pub client_id: Option<&'a str>,
    pub authorized_party: Option<&'a str>,
    pub ttl_seconds: u64,
}

#[derive(Serialize)]
struct JwtHeader {
    alg: &'static str,
    typ: &'static str,
    kid: String,
}

#[derive(Serialize)]
struct AccessTokenClaims {
    iss: String,
    sub: String,
    aud: String,
    exp: i64,
    iat: i64,
    jti: String,
    scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    roles: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    azp: Option<String>,
}

fn encode_json<T: Serialize>(value: &T) -> Result<String> {
    let json = serde_json::to_vec(value).map_err(|error| AppError::Token(error.to_string()))?;
    Ok(URL_SAFE_NO_PAD.encode(json))
}
