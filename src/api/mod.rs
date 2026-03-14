use crate::AppState;
use axum::Router;

mod extractors;
pub mod internal;
pub mod public;
mod utils;

pub fn create_router(state: AppState) -> Router {
    Router::new()
        .nest("/v1", public::router(state.clone()))
        .nest("/internal", internal::router(state))
}
