use sqlx::PgPool;

use crate::token::TokenService;

#[derive(Clone)]
pub struct AppState {
    pub db_pool: PgPool,
    pub secure_cookies: bool,
    pub issuer: String,
    pub token_service: TokenService,
}

impl AppState {
    pub fn new(
        db_pool: PgPool,
        secure_cookies: bool,
        issuer: String,
        token_service: TokenService,
    ) -> Self {
        Self {
            db_pool,
            secure_cookies,
            issuer,
            token_service,
        }
    }
}
