use crate::state::AppState;
use axum::Router;

pub fn build_app(state: AppState) -> Router {
    Router::new()
        .merge(crate::routes::router())
        .with_state(state)
}
