pub mod app;
pub mod config;
pub mod db;
pub mod error;
pub mod identity;
pub mod state;

pub mod http {
    pub mod auth;
    pub mod health;
    pub mod router;
}
