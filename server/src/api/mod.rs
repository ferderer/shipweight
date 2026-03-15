pub mod badge;
pub mod common;
pub mod compat;
pub mod health;
pub mod v1;

use std::sync::Arc;

use axum::Router;

use crate::cache::CacheService;

#[derive(Clone)]
pub struct AppState {
    pub cache: Arc<CacheService>,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .merge(health::router(state.clone()))
        .merge(v1::router(state.clone()))
        .merge(badge::router(state.clone()))
        .merge(compat::router(state))
}
