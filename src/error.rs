use thiserror::Error;

pub type Result<T> = std::result::Result<T, AppError>;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("missing required environment variable: {0}")]
    MissingEnvVar(&'static str),
    #[error("invalid numeric value for {name}: {value}")]
    InvalidNumber { name: &'static str, value: String },
    #[error("password hash error: {0}")]
    PasswordHash(String),
    #[error("invalid bind address: {0}")]
    InvalidBindAddress(#[from] std::net::AddrParseError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
}

impl From<argon2::password_hash::Error> for AppError {
    fn from(value: argon2::password_hash::Error) -> Self {
        Self::PasswordHash(value.to_string())
    }
}
