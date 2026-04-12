use std::{env, net::SocketAddr};

use crate::error::Result;

const DEFAULT_BIND_ADDR: &str = "127.0.0.1:3000";

#[derive(Debug, Clone)]
pub struct Config {
    pub bind_addr: SocketAddr,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let bind_addr = env::var("AUTH_SERVER_BIND_ADDR")
            .unwrap_or_else(|_| DEFAULT_BIND_ADDR.to_owned())
            .parse()?;

        Ok(Self { bind_addr })
    }
}
