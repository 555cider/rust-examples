use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use crate::{config, entity};

pub type ThrottlePool = Arc<Mutex<ThrottleState>>;

#[derive(Debug, Default)]
pub struct ThrottleState {
    entries: HashMap<(String, String), ThrottleWindow>,
}

#[derive(Debug)]
struct ThrottleWindow {
    started_at: Instant,
    count: u64,
}

pub fn init_throttle() -> std::io::Result<ThrottlePool> {
    init_throttle_at("throttle.db")
}

pub fn init_throttle_at(_path: impl AsRef<Path>) -> std::io::Result<ThrottlePool> {
    Ok(Arc::new(Mutex::new(ThrottleState::default())))
}

pub fn run(
    throttle_state: &ThrottlePool,
    route: &config::Route,
    headers: &hyper::HeaderMap,
) -> Result<hyper::http::StatusCode, entity::GatewayError> {
    let Some(rate_limit) = route.rate_limit.as_ref() else {
        return Ok(hyper::http::StatusCode::OK);
    };

    if rate_limit.requests == 0 {
        return Err(entity::GatewayError::InvalidRouteConfig(format!(
            "route '{}' rate_limit.requests must be greater than 0",
            route.path
        )));
    }
    if rate_limit.window_ms == 0 {
        return Err(entity::GatewayError::InvalidRouteConfig(format!(
            "route '{}' rate_limit.window_ms must be greater than 0",
            route.path
        )));
    }

    let header_name = hyper::header::HeaderName::from_bytes(rate_limit.key_header.as_bytes())
        .map_err(|err| {
            entity::GatewayError::InvalidRouteConfig(format!(
                "invalid rate_limit.key_header '{}' for route '{}': {}",
                rate_limit.key_header, route.path, err
            ))
        })?;

    let Some(value) = headers.get(&header_name) else {
        return Ok(hyper::http::StatusCode::BAD_REQUEST);
    };
    let Ok(value) = value.to_str() else {
        return Ok(hyper::http::StatusCode::BAD_REQUEST);
    };

    let window = Duration::from_millis(rate_limit.window_ms);
    let now = Instant::now();
    let mut throttle_state = throttle_state.lock().unwrap();
    let entry = throttle_state
        .entries
        .entry((route.path.clone(), value.to_string()))
        .or_insert_with(|| ThrottleWindow {
            started_at: now,
            count: 0,
        });

    if now.duration_since(entry.started_at) >= window {
        entry.started_at = now;
        entry.count = 0;
    }

    if entry.count >= rate_limit.requests {
        return Ok(hyper::http::StatusCode::TOO_MANY_REQUESTS);
    }

    entry.count += 1;
    Ok(hyper::http::StatusCode::OK)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initializes_named_throttle_store_without_db_artifacts() {
        let db_path = std::env::temp_dir().join(format!("throttle-{}.db", fastrand::u64(..)));
        let pool = init_throttle_at(&db_path).expect("throttle store should initialize");
        assert!(pool.lock().is_ok());
        drop(pool);
        for path in [
            db_path.clone(),
            std::path::PathBuf::from(format!("{}-journal", db_path.display())),
            std::path::PathBuf::from(format!("{}-shm", db_path.display())),
            std::path::PathBuf::from(format!("{}-wal", db_path.display())),
        ] {
            assert!(
                !path.exists(),
                "temporary throttle artifact should not exist: {}",
                path.display()
            );
        }
    }
}
