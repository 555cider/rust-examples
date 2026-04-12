use sqlx::PgPool;

#[derive(Clone)]
pub struct AppState {
    pub db_pool: PgPool,
    pub secure_cookies: bool,
}

impl AppState {
    pub fn new(db_pool: PgPool, secure_cookies: bool) -> Self {
        Self {
            db_pool,
            secure_cookies,
        }
    }
}
