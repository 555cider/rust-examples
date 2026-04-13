use axum::{Json, Router, extract::State, http::StatusCode, routing::post};

use crate::{policy, state::AppState};

pub fn build_router() -> Router<AppState> {
    Router::new().route("/policy/evaluate", post(evaluate))
}

async fn evaluate(
    State(state): State<AppState>,
    Json(request): Json<policy::EvaluateRequest>,
) -> Result<Json<policy::EvaluateResponse>, StatusCode> {
    let response = policy::evaluate(&state.db_pool, request)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(response))
}
