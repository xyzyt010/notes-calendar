//! yellod suite — notes & calendar web app backend.
//! Efficient Rust (axum + SQLite via ycore). Serves static frontends for
//! /notes and /calendar plus a JSON API (both apps use it).
//! Auth: opaque random session tokens (httpOnly cookie), verified server-side.
//! Efficient: zero crypto beyond bcrypt for the login itself.
//!
//! Calendar-only deployments (e.g. calendar.yellod.dpdns.org): set
//! SUITE_DISABLE_NOTES=1 — the Notes static page, Notes API and notes routes
//! are not served; / redirects to the calendar page.

use axum::Router;
use tower_http::services::ServeDir;

mod auth;
mod calendar;
mod notes;
mod state;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env()
            .add_directive("suite_web=info".parse().unwrap()))
        .init();

    let state = state::AppState::from_env()?;
    let bind_addr = state.bind_addr;
    let disable_notes = std::env::var("SUITE_DISABLE_NOTES")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("yes"))
        .unwrap_or(false);
    tracing::info!("yellod suite listening on {} (calendar-only={})", bind_addr, disable_notes);

    let index_target: &'static str = if disable_notes { "/calendar.html" } else { "/index.html" };
    let mut app = Router::new()
        .route("/", axum::routing::get(move || async move { axum::response::Redirect::to(index_target) }))
        .route("/calendar", axum::routing::get(|| async { axum::response::Redirect::to("/calendar.html") }))
        .route("/api/health", axum::routing::get(|| async { axum::Json(serde_json::json!({"ok": true})) }))
        .route("/api/auth/login", axum::routing::post(auth::login))
        .route("/api/auth/logout", axum::routing::post(auth::logout))
        .route("/api/auth/me", axum::routing::get(auth::me))
        .merge(calendar::routes());

    if disable_notes {
        // Public calendar deployment: no Notes anywhere. Old links redirect to the calendar.
        app = app
            .route("/notes", axum::routing::get(|| async { axum::response::Redirect::to("/calendar.html") }))
            .route("/notes.html", axum::routing::get(|| async { axum::response::Redirect::to("/calendar.html") }));
        // SUITE_DISABLE_NOTES=1: Notes API is not mounted. Deployments should also remove static/notes.html.
    } else {
        app = app
            .route("/notes", axum::routing::get(|| async { axum::response::Redirect::to("/notes.html") }))
            .merge(notes::routes());
    }

    let app = app
        .fallback_service(ServeDir::new(&state.static_dir).append_index_html_on_directories(true))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
