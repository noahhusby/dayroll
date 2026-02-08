use axum::Router;
use crate::state::AppState;

pub fn build_app(state: AppState) -> Router {
    Router::new()
        .merge(crate::routes::router())
        .with_state(state)
}
