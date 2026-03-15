mod api;
mod cache;
mod common;
mod config;

use std::sync::Arc;

use axum::routing::get;
use axum_prometheus::PrometheusMetricLayer;
use tower_http::cors::{Any, CorsLayer};

use crate::api::AppState;
use crate::cache::CacheService;

#[tokio::main]
async fn main() {
    config::tracing::init();

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = config::database::connect(&database_url).await;

    let cache = Arc::new(CacheService::new(pool));

    // Flush request metrics to Postgres every 5 minutes
    let metrics_cache = cache.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));
        loop {
            interval.tick().await;
            metrics_cache.flush_request_stats().await;
        }
    });

    // Reset stale jobs every 5 minutes (matches 5-minute processing timeout)
    let stale_cache = cache.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));
        loop {
            interval.tick().await;
            stale_cache.reset_stale_jobs().await;
        }
    });

    // Public API — allow any origin (read-only endpoints, no credentials)
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let (prometheus_layer, metric_handle) = PrometheusMetricLayer::pair();

    let state = AppState { cache };
    let app = api::router(state)
        .route("/metrics", get(move || async move { metric_handle.render() }))
        .layer(prometheus_layer)
        .layer(cors);

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3000);

    config::server::run(app, port).await;
}
