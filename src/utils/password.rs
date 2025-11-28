use crate::errors::AppError;
use bcrypt::{hash, verify, DEFAULT_COST};
use tokio::task::spawn_blocking;

/// 비밀번호 해싱
///
/// # Security Notes
/// - Uses bcrypt with DEFAULT_COST (currently 12 rounds)
/// - Automatically generates a random salt
/// - Runs in a blocking thread pool to avoid blocking the async runtime
/// - The cost factor makes the hash computation deliberately slow to resist brute force attacks
pub async fn hash_password(password: &str) -> Result<String, AppError> {
    let password_bytes = password.as_bytes().to_vec(); // bcrypt는 비동기 아님, 스레드 풀에서 실행
    spawn_blocking(move || hash(password_bytes, DEFAULT_COST))
        .await
        .map_err(|_e| AppError::InternalServerError("Password hashing task failed".to_string()))?
        .map_err(AppError::PasswordHashingError)
}

/// 비밀번호 검증
///
/// # Security Notes
/// - Uses bcrypt which provides constant-time comparison to prevent timing attacks
/// - Runs in a blocking thread pool to avoid blocking the async runtime
/// - Password is converted to bytes for bcrypt compatibility
pub async fn verify_password(password: &str, hash: &str) -> Result<bool, AppError> {
    let password_bytes = password.as_bytes().to_vec();
    let hash_str = hash.to_string(); // 해시값 복사
    spawn_blocking(move || verify(password_bytes, &hash_str))
        .await
        .map_err(|_e| {
            AppError::InternalServerError("Password verification task failed".to_string())
        })?
        .map_err(AppError::PasswordHashingError)
}
