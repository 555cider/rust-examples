use rand::{RngCore, rngs::OsRng};
use sha2::{Digest, Sha256};

pub struct SessionToken {
    pub plain_text: String,
    pub hash: String,
}

pub fn issue_session_token() -> SessionToken {
    let mut bytes = [0_u8; 32];
    OsRng.fill_bytes(&mut bytes);

    let plain_text = hex::encode(bytes);
    let hash = hash_session_token(&plain_text);

    SessionToken { plain_text, hash }
}

pub fn hash_session_token(session_token: &str) -> String {
    let digest = Sha256::digest(session_token.as_bytes());
    hex::encode(digest)
}
