use super::access_control::AccessControlConfig;
use dotenvy::from_filename;
use env::var;
use std::env;
use std::path::Path;
use std::sync::OnceLock;

static CONFIG: OnceLock<AppConfig> = OnceLock::new();

pub fn get_config() -> &'static AppConfig {
    CONFIG.get_or_init(AppConfig::from_env)
}

#[derive(Clone)]
pub struct AppConfig {
    pub app_name: String,
    pub database_url: String,
    pub server_host: String,
    pub server_port: u16,
    pub log: Log,
    pub token: Token,
    pub cookie: Cookie,
    pub access_control: AccessControlConfig,
}

impl AppConfig {
    pub fn from_env() -> Self {
        load_env_files();

        // Validate critical environment variables with helpful error messages
        let database_url = var("DATABASE_URL").unwrap_or_else(|_| {
            panic!(
                "FATAL: DATABASE_URL environment variable is not set!\n\
                     Please set it in your .env file or environment.\n\
                     Example: DATABASE_URL=sqlite:data/sqlite.db"
            );
        });

        let server_host = var("SERVER_HOST").unwrap_or_else(|_| {
            eprintln!("ERROR: SERVER_HOST environment variable is not set!");
            eprintln!("Using default: 127.0.0.1");
            "127.0.0.1".to_string()
        });

        let server_port = var("SERVER_PORT")
            .unwrap_or_else(|_| {
                eprintln!("WARNING: SERVER_PORT not set, using default: 8080");
                "8080".to_string()
            })
            .parse::<u16>()
            .unwrap_or_else(|e| {
                eprintln!("ERROR: SERVER_PORT must be a valid number (1-65535): {}. Using default: 8080", e);
                8080
            });

        Self {
            app_name: var("APP_NAME").unwrap_or("admin-server".to_string()),
            database_url,
            server_host,
            server_port,
            log: Log::from_env(),
            token: Token::from_env(),
            cookie: Cookie::from_env(),
            access_control: AccessControlConfig::from_env(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Log {
    pub console_enable: bool,
    pub console_level: String, // "debug", "info", "warn", "error"
    pub file_enable: bool,
    pub file_path: String,
    pub file_level: String, // "debug", "info", "warn", "error"
    pub file_rotation: String, // "daily", "hourly", "never"
    pub file_max_files: Option<usize>, // Maximum number of log files to keep
}

impl Log {
    pub fn from_env() -> Self {
        Self {
            console_enable: var("LOG_CONSOLE_ENABLE")
                .unwrap_or("true".to_string())
                .parse()
                .unwrap_or_else(|e| {
                    eprintln!("WARNING: LOG_CONSOLE_ENABLE must be a valid boolean: {}. Using default: true", e);
                    true
                }),
            console_level: var("RUST_LOG").unwrap_or("info".to_string()),
            file_enable: var("LOG_FILE_ENABLE")
                .unwrap_or("true".to_string())
                .parse()
                .unwrap_or_else(|e| {
                    eprintln!("WARNING: LOG_FILE_ENABLE must be a valid boolean: {}. Using default: true", e);
                    true
                }),
            file_level: var("LOG_FILE_LEVEL").unwrap_or("info".to_string()),
            file_path: var("LOG_FILE_PATH").unwrap_or("logs".to_string()),
            file_rotation: var("LOG_FILE_ROTATION").unwrap_or("daily".to_string()),
            file_max_files: var("LOG_FILE_MAX_FILES")
                .ok()
                .and_then(|v| v.parse().ok()),
        }
    }
}

#[derive(Clone)]
pub struct Token {
    pub secret: String,
    pub access_name: String,
    pub access_exp: i64,
    pub refresh_name: String,
    pub refresh_exp: i64,
}

impl Token {
    pub fn from_env() -> Self {
        let secret = var("TOKEN_SECRET").unwrap_or_else(|_| {
            panic!(
                "FATAL: TOKEN_SECRET environment variable is not set!\n\
                 This is required for JWT authentication security.\n\
                 Generate a strong secret with: openssl rand -base64 64"
            );
        });

        // Validate token secret strength
        if secret.len() < 32 || secret.contains("CHANGE_IN_PRODUCTION") {
            eprintln!("WARNING: TOKEN_SECRET appears to be weak or using default value!");
            eprintln!("For production use, generate a strong secret with: openssl rand -base64 64");
        }

        Self {
            secret,
            access_name: var("TOKEN_ACCESS_NAME").unwrap_or("access".to_string()),
            access_exp: var("TOKEN_ACCESS_EXP")
                .unwrap_or("3600".to_string())
                .parse()
                .unwrap_or_else(|e| {
                    eprintln!("WARNING: TOKEN_ACCESS_EXP must be a valid number: {}. Using default: 3600 seconds", e);
                    3600
                }),
            refresh_name: var("TOKEN_REFRESH_NAME").unwrap_or("refresh".to_string()),
            refresh_exp: var("TOKEN_REFRESH_EXP")
                .unwrap_or("86400".to_string())
                .parse()
                .unwrap_or_else(|e| {
                    eprintln!("WARNING: TOKEN_REFRESH_EXP must be a valid number: {}. Using default: 86400 seconds", e);
                    86400
                }),
        }
    }
}

#[derive(Clone)]
pub struct Cookie {
    pub access_token_name: String,
    pub access_token_max_age: i64,
    pub refresh_token_name: String,
    pub refresh_token_max_age: i64,
    pub domain: String,
    pub secure: bool,
    pub http_only: bool,
    pub same_site: String,
}

impl Cookie {
    pub fn from_env() -> Self {
        let secure = var("COOKIE_SECURE")
            .unwrap_or("false".to_string())
            .parse()
            .unwrap_or_else(|e| {
                eprintln!("WARNING: COOKIE_SECURE must be a valid boolean: {}. Using default: false", e);
                false
            });

        let http_only = var("COOKIE_HTTP_ONLY")
            .unwrap_or("false".to_string())
            .parse()
            .unwrap_or_else(|e| {
                eprintln!("WARNING: COOKIE_HTTP_ONLY must be a valid boolean: {}. Using default: false", e);
                false
            });

        // Security warning for production
        if !http_only {
            eprintln!("WARNING: COOKIE_HTTP_ONLY is disabled. For production, enable it to prevent XSS attacks.");
        }
        if !secure {
            eprintln!("WARNING: COOKIE_SECURE is disabled. For production over HTTPS, enable it.");
        }

        Self {
            access_token_name: var("COOKIE_ACCESS_TOKEN_NAME")
                .unwrap_or("access_token".to_string()),
            access_token_max_age: var("COOKIE_ACCESS_TOKEN_MAX_AGE")
                .unwrap_or("3600".to_string())
                .parse()
                .unwrap_or_else(|e| {
                    eprintln!(
                        "WARNING: COOKIE_ACCESS_TOKEN_MAX_AGE must be a valid number: {}",
                        e
                    );
                    eprintln!("Using default: 3600 seconds");
                    3600
                }),
            refresh_token_name: var("COOKIE_REFRESH_TOKEN_NAME")
                .unwrap_or("refresh_token".to_string()),
            refresh_token_max_age: var("COOKIE_REFRESH_TOKEN_MAX_AGE")
                .unwrap_or("86400".to_string())
                .parse()
                .unwrap_or_else(|e| {
                    eprintln!(
                        "WARNING: COOKIE_REFRESH_TOKEN_MAX_AGE must be a valid number: {}",
                        e
                    );
                    eprintln!("Using default: 86400 seconds");
                    86400
                }),
            domain: var("COOKIE_DOMAIN").unwrap_or("localhost".to_string()),
            secure,
            http_only,
            same_site: var("COOKIE_SAME_SITE").unwrap_or("lax".to_string()),
        }
    }
}

fn load_env_files() {
    // 환경 확인
    let rust_env = var("RUST_ENV").unwrap_or_else(|_| "dev".to_string());

    let env_files = vec![
        ".env".to_string(),           // 1. 기본값
        format!(".env.{}", rust_env), // 2. 환경별
    ];

    for file in env_files {
        if Path::new(&file).exists() {
            if let Err(e) = from_filename(&file) {
                eprintln!("Failed to load configuration from {}: {}", file, e);
            }
        }
    }
}
