//! Auth: opaque random session tokens, verified server-side.
//! No JWT, no external crypto crates — a single-user self-hosted app.

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::Deserialize;

use crate::state::AppState;

const SESSION_COOKIE: &str = "suite_session";

#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

fn new_token() -> String {
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes).expect("getrandom failed");
    URL_SAFE_NO_PAD.encode(bytes)
}

pub async fn login(State(state): State<AppState>, Json(body): Json<LoginRequest>) -> impl IntoResponse {
    if body.username != "admin" || !bcrypt::verify(&body.password, &state.admin_password_hash).unwrap_or(false) {
        return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "invalid credentials"}))).into_response();
    }
    let token = new_token();
    state.sessions.lock().unwrap().insert(token.clone());
    let cookie = format!("{SESSION_COOKIE}={token}; HttpOnly; SameSite=Lax; Path=/; Max-Age=2592000");
    let mut resp = Json(serde_json::json!({"ok": true})).into_response();
    resp.headers_mut()
        .insert(axum::http::header::SET_COOKIE, axum::http::HeaderValue::from_str(&cookie).unwrap());
    resp
}

pub async fn logout(State(state): State<AppState>, headers: axum::http::HeaderMap) -> impl IntoResponse {
    if let Some(token) = jwt_from_cookie(&headers) {
        state.sessions.lock().unwrap().remove(&token);
    }
    let cookie = format!("{SESSION_COOKIE}=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0");
    let mut resp = Json(serde_json::json!({"ok": true})).into_response();
    resp.headers_mut()
        .insert(axum::http::header::SET_COOKIE, axum::http::HeaderValue::from_str(&cookie).unwrap());
    resp
}

#[derive(serde::Serialize)]
pub struct MeResponse {
    pub username: String,
}

pub async fn me(State(state): State<AppState>, headers: axum::http::HeaderMap) -> impl IntoResponse {
    if require_auth(&state, &headers).is_ok() {
        Json(MeResponse { username: "admin".into() }).into_response()
    } else {
        StatusCode::UNAUTHORIZED.into_response()
    }
}

/// Extract the session token from the Cookie header.
pub fn jwt_from_cookie(req_headers: &axum::http::HeaderMap) -> Option<String> {
    let cookie = req_headers.get(axum::http::header::COOKIE)?.to_str().ok()?;
    cookie.split(';').find_map(|part| {
        let part = part.trim();
        part.strip_prefix(&format!("{SESSION_COOKIE}="))
            .map(|s| s.to_string())
    })
}

/// Verify the request's cookie belongs to a live session.
pub fn require_auth(state: &AppState, headers: &axum::http::HeaderMap) -> Result<(), StatusCode> {
    let Some(token) = jwt_from_cookie(headers) else {
        return Err(StatusCode::UNAUTHORIZED);
    };
    if state.sessions.lock().unwrap().contains(&token) {
        Ok(())
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}
