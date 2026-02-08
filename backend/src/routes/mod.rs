use crate::state::AppState;
use axum::Router;

pub mod health;

pub fn router() -> Router<AppState> {
    Router::new().nest("/health", health::router())
}
