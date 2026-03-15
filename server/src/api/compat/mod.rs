use std::time::Duration;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{Json, Router};
use axum::routing::get;
use serde::Deserialize;

use crate::common::error::AppError;
use super::AppState;
use super::common::response::BundlephobiaResponse;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/size", get(get_size_compat))
        .with_state(state)
}

#[derive(Deserialize)]
struct SizeQuery {
    package: String,
}

/// Bundlephobia-compatible endpoint.
/// Synchronous: enqueues job, then polls size_cache for up to 30s.
async fn get_size_compat(
    State(state): State<AppState>,
    Query(query): Query<SizeQuery>,
) -> Response {
    let (name, version) = match parse_package_spec(&query.package) {
        Ok(pair) => pair,
        Err(e) => return e.into_response(),
    };

    // 1. Check cache
    if let Ok(Some(result)) = state.cache.get("npm", &name, &version).await {
        state.cache.track_request("npm", &name);
        return (StatusCode::OK, Json(BundlephobiaResponse::from(result))).into_response();
    }

    // 2. Check failed_packages
    if let Some(error) = state.cache.get_failure("npm", &name, &version).await {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({ "error": error })),
        )
            .into_response();
    }

    // 3. Enqueue job
    if let Err(e) = state.cache.enqueue_job("npm", &name, &version).await {
        tracing::warn!(error = %e, name = %name, version = %version, "failed to enqueue job");
    }

    // 4. Poll size_cache every 1s for up to 30s
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    let mut interval = tokio::time::interval(Duration::from_secs(1));

    loop {
        interval.tick().await;

        if tokio::time::Instant::now() >= deadline {
            return (
                StatusCode::GATEWAY_TIMEOUT,
                Json(serde_json::json!({ "error": "analysis timed out" })),
            )
                .into_response();
        }

        if let Ok(Some(result)) = state.cache.get("npm", &name, &version).await {
            state.cache.track_request("npm", &name);
            return (StatusCode::OK, Json(BundlephobiaResponse::from(result))).into_response();
        }

        // Check if the job failed while we were waiting
        if let Some(error) = state.cache.get_failure("npm", &name, &version).await {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(serde_json::json!({ "error": error })),
            )
                .into_response();
        }
    }
}

fn parse_package_spec(spec: &str) -> Result<(String, String), AppError> {
    if spec.starts_with('@') {
        if let Some(at_pos) = spec[1..].find('@') {
            let at_pos = at_pos + 1;
            let name = &spec[..at_pos];
            let version = &spec[at_pos + 1..];
            if name.is_empty() || version.is_empty() {
                return Err(AppError::NotFound("invalid package specifier".into()));
            }
            return Ok((name.to_string(), version.to_string()));
        }
    } else if let Some(at_pos) = spec.find('@') {
        let name = &spec[..at_pos];
        let version = &spec[at_pos + 1..];
        if name.is_empty() || version.is_empty() {
            return Err(AppError::NotFound("invalid package specifier".into()));
        }
        return Ok((name.to_string(), version.to_string()));
    }

    Err(AppError::NotFound(
        "package specifier must be in format name@version".into(),
    ))
}
