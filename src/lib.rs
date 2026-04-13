pub mod admin;
pub mod app;
pub mod audit;
pub mod config;
pub mod db;
pub mod error;
pub mod identity;
pub mod oauth;
pub mod policy;
pub mod state;
pub mod token;

pub mod http {
    pub mod admin;
    pub mod auth;
    pub mod health;
    pub mod oauth;
    pub mod oidc;
    pub mod policy;
    pub mod router;
}
