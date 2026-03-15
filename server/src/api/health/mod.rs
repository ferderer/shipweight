use axum::{Json, Router};
use axum::extract::State;
use axum::routing::get;
use serde::Serialize;

use super::AppState;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .with_state(state)
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    version: &'static str,
    cache_entries: u64,
}

async fn health_check(State(state): State<AppState>) -> Json<HealthResponse> {
    let stats = state.cache.stats();
    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
        cache_entries: stats.l1_entries,
    })
}
