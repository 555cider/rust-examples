use std::{env, net::SocketAddr, path::PathBuf};

use crate::error::{AppError, Result};

const DEFAULT_BIND_ADDR: &str = "127.0.0.1:3000";
const DEFAULT_DATABASE_MAX_CONNECTIONS: u32 = 10;

#[derive(Debug, Clone)]
pub struct Config {
    pub bind_addr: SocketAddr,
    pub database_url: String,
    pub database_max_connections: u32,
    pub issuer: String,
    pub signing_key_path: Option<PathBuf>,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let bind_addr = env::var("AUTH_SERVER_BIND_ADDR")
            .unwrap_or_else(|_| DEFAULT_BIND_ADDR.to_owned())
            .parse()?;
        let database_url = env::var("AUTH_SERVER_DATABASE_URL")
            .map_err(|_| AppError::MissingEnvVar("AUTH_SERVER_DATABASE_URL"))?;
        let database_max_connections = match env::var("AUTH_SERVER_DATABASE_MAX_CONNECTIONS") {
            Ok(value) => value.parse().map_err(|_| AppError::InvalidNumber {
                name: "AUTH_SERVER_DATABASE_MAX_CONNECTIONS",
                value,
            })?,
            Err(_) => DEFAULT_DATABASE_MAX_CONNECTIONS,
        };
        let issuer =
            env::var("AUTH_SERVER_ISSUER").unwrap_or_else(|_| format!("http://{bind_addr}"));
        let signing_key_path = env::var("AUTH_SERVER_SIGNING_KEY_PATH")
            .ok()
            .map(PathBuf::from);

        Ok(Self {
            bind_addr,
            database_url,
            database_max_connections,
            issuer,
            signing_key_path,
        })
    }
}
