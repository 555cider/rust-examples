use std::{env, net::SocketAddr};

use crate::error::{AppError, Result};

const DEFAULT_BIND_ADDR: &str = "127.0.0.1:3000";
const DEFAULT_DATABASE_MAX_CONNECTIONS: u32 = 10;

#[derive(Debug, Clone)]
pub struct Config {
    pub bind_addr: SocketAddr,
    pub database_url: String,
    pub database_max_connections: u32,
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

        Ok(Self {
            bind_addr,
            database_url,
            database_max_connections,
        })
    }
}
