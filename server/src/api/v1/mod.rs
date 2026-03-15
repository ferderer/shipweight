use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{Json, Router};
use axum::routing::get;
use serde::Deserialize;

use super::AppState;
use super::common::response::{FailedResponse, QueuedResponse, ReadyResponse};
use crate::common::validate::invalid_npm_name;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/v1/{ecosystem}/{package}/{version}",            get(get_size))
        .route("/v1/{ecosystem}/@{scope}/{package}/{version}",   get(get_scoped_size))
        .route("/v1/{ecosystem}/{package}",                      get(get_latest))
        .route("/v1/{ecosystem}/@{scope}/{package}",             get(get_scoped_latest))
        // Prefixed with ~ to avoid conflicting with {version} segment
        .route("/v1/{ecosystem}/{package}/~alternatives",        get(get_alternatives))
        .route("/v1/{ecosystem}/@{scope}/{package}/~alternatives", get(get_scoped_alternatives))
        .route("/v1/{ecosystem}/search",                         get(search))
        .route("/v1/{ecosystem}/top",                            get(top))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn validate_name(ecosystem: &str, package: &str) -> Option<Response> {
    match ecosystem {
        "npm" => {
            if let Some(reason) = invalid_npm_name(package) {
                return Some((
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({ "error": format!("invalid package name: {reason}") })),
                ).into_response());
            }
        }
        other => {
            return Some((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": format!("unknown ecosystem: {other}") })),
            ).into_response());
        }
    }
    None
}

fn internal_error(e: impl std::fmt::Display) -> Response {
    (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response()
}

// ---------------------------------------------------------------------------
// Version-specific
// ---------------------------------------------------------------------------

async fn get_size(
    State(state): State<AppState>,
    Path((ecosystem, package, version)): Path<(String, String, String)>,
) -> Response {
    handle_size_request(state, &ecosystem, &package, &version).await
}

async fn get_scoped_size(
    State(state): State<AppState>,
    Path((ecosystem, scope, package, version)): Path<(String, String, String, String)>,
) -> Response {
    handle_size_request(state, &ecosystem, &format!("@{scope}/{package}"), &version).await
}

async fn handle_size_request(state: AppState, ecosystem: &str, package: &str, version: &str) -> Response {
    if let Some(err) = validate_name(ecosystem, package) {
        return err;
    }

    match state.cache.get(ecosystem, package, version).await {
        Err(e) => return internal_error(e),
        Ok(Some(result)) => {
            state.cache.track_request(ecosystem, package);
            return (StatusCode::OK, Json(ReadyResponse { status: "ready", result })).into_response();
        }
        Ok(None) => {}
    }

    if let Some(error) = state.cache.get_failure(ecosystem, package, version).await {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(FailedResponse {
                status: "failed",
                name: package.to_string(),
                version: version.to_string(),
                error,
            }),
        ).into_response();
    }

    if let Err(e) = state.cache.enqueue_job(ecosystem, package, version).await {
        tracing::warn!(error = %e, package, version, "failed to enqueue job");
    }

    (
        StatusCode::ACCEPTED,
        [("Retry-After", "2")],
        Json(QueuedResponse {
            status: "processing",
            name: package.to_string(),
            version: version.to_string(),
            retry_after: 2,
        }),
    ).into_response()
}

// ---------------------------------------------------------------------------
// Latest
// ---------------------------------------------------------------------------

async fn get_latest(
    State(state): State<AppState>,
    Path((ecosystem, package)): Path<(String, String)>,
) -> Response {
    handle_latest_request(state, &ecosystem, &package).await
}

async fn get_scoped_latest(
    State(state): State<AppState>,
    Path((ecosystem, scope, package)): Path<(String, String, String)>,
) -> Response {
    handle_latest_request(state, &ecosystem, &format!("@{scope}/{package}")).await
}

async fn handle_latest_request(state: AppState, ecosystem: &str, package: &str) -> Response {
    if let Some(err) = validate_name(ecosystem, package) {
        return err;
    }

    match state.cache.get_latest(ecosystem, package).await {
        Ok(Some(result)) => {
            state.cache.track_request(ecosystem, package);
            (StatusCode::OK, Json(result)).into_response()
        }
        Ok(None) => (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "package not found" }))).into_response(),
        Err(e) => internal_error(e),
    }
}

// ---------------------------------------------------------------------------
// Search
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct SearchParams {
    q: Option<String>,
    keyword: Option<String>,
    treeshakeable: Option<bool>,
    has_types: Option<bool>,
    sort: Option<String>,
    order: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

async fn search(
    State(state): State<AppState>,
    Path(ecosystem): Path<String>,
    Query(params): Query<SearchParams>,
) -> Response {
    if let Some(err) = validate_name(&ecosystem, "_dummy_scoped_check_skipped") {
        // Only check ecosystem validity, not package name
        let _ = err;
    }
    // Validate ecosystem directly
    if ecosystem.parse::<crate::common::types::Ecosystem>().is_err() {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": format!("unknown ecosystem: {ecosystem}") }))).into_response();
    }

    let limit = params.limit.unwrap_or(20).clamp(1, 100);
    let offset = params.offset.unwrap_or(0).max(0);

    match state.cache.search(
        &ecosystem,
        params.q.as_deref(),
        params.keyword.as_deref(),
        params.treeshakeable,
        params.has_types,
        params.sort.as_deref().unwrap_or("gzip"),
        params.order.as_deref().unwrap_or("asc"),
        limit,
        offset,
    ).await {
        Ok(results) => (StatusCode::OK, Json(results)).into_response(),
        Err(e) => internal_error(e),
    }
}

// ---------------------------------------------------------------------------
// Top
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct TopParams {
    sort: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

async fn top(
    State(state): State<AppState>,
    Path(ecosystem): Path<String>,
    Query(params): Query<TopParams>,
) -> Response {
    if ecosystem.parse::<crate::common::types::Ecosystem>().is_err() {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": format!("unknown ecosystem: {ecosystem}") }))).into_response();
    }

    let limit = params.limit.unwrap_or(20).clamp(1, 100);
    let offset = params.offset.unwrap_or(0).max(0);

    match state.cache.top(&ecosystem, params.sort.as_deref().unwrap_or("downloads"), limit, offset).await {
        Ok(results) => (StatusCode::OK, Json(results)).into_response(),
        Err(e) => internal_error(e),
    }
}

// ---------------------------------------------------------------------------
// Alternatives
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct AlternativesParams {
    limit: Option<i64>,
}

async fn get_alternatives(
    State(state): State<AppState>,
    Path((ecosystem, package)): Path<(String, String)>,
    Query(params): Query<AlternativesParams>,
) -> Response {
    handle_alternatives(state, &ecosystem, &package, params.limit).await
}

async fn get_scoped_alternatives(
    State(state): State<AppState>,
    Path((ecosystem, scope, package)): Path<(String, String, String)>,
    Query(params): Query<AlternativesParams>,
) -> Response {
    handle_alternatives(state, &ecosystem, &format!("@{scope}/{package}"), params.limit).await
}

async fn handle_alternatives(state: AppState, ecosystem: &str, package: &str, limit: Option<i64>) -> Response {
    if let Some(err) = validate_name(ecosystem, package) {
        return err;
    }

    let limit = limit.unwrap_or(10).clamp(1, 50);

    match state.cache.alternatives(ecosystem, package, limit).await {
        Ok(results) => (StatusCode::OK, Json(serde_json::json!({
            "package": package,
            "alternatives": results,
        }))).into_response(),
        Err(e) => internal_error(e),
    }
}
