use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::extract::State;
use axum::http::{Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Redirect, Response};
use axum::Form;
use serde::Deserialize;
use tower_sessions::Session;

use crate::state::{AppState, Config};

const SESSION_USER_KEY: &str = "authenticated";

/// Hash a password with Argon2id
pub fn hash_password(password: &str) -> Result<String, String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    argon2
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| format!("Failed to hash password: {}", e))
}

/// Verify a password against an Argon2id hash
pub fn verify_password(password: &str, hash: &str) -> bool {
    let parsed = match PasswordHash::new(hash) {
        Ok(h) => h,
        Err(_) => return false,
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

// ---- Middleware ----

/// Auth middleware — redirects to /setup if unconfigured, /login if unauthenticated
pub async fn auth_middleware(
    State(state): State<AppState>,
    session: Session,
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    if !state.is_configured().await {
        return Redirect::to("/setup").into_response();
    }

    let is_authenticated = session
        .get::<bool>(SESSION_USER_KEY)
        .await
        .unwrap_or(None)
        .unwrap_or(false);

    if is_authenticated {
        next.run(request).await
    } else {
        Redirect::to("/login").into_response()
    }
}

// ---- Setup ----

#[derive(askama::Template, askama_web::WebTemplate)]
#[template(path = "setup.html")]
pub struct SetupTemplate {
    pub error: Option<String>,
}

#[derive(Deserialize)]
pub struct SetupForm {
    pub username: String,
    pub password: String,
    pub password_confirm: String,
    pub zt_token: String,
}

/// GET /setup
pub async fn setup_page(State(state): State<AppState>) -> Response {
    if state.is_configured().await {
        return Redirect::to("/login").into_response();
    }
    SetupTemplate { error: None }.into_response()
}

/// POST /setup
pub async fn setup_submit(
    State(state): State<AppState>,
    Form(form): Form<SetupForm>,
) -> Response {
    if state.is_configured().await {
        return Redirect::to("/login").into_response();
    }

    let username = form.username.trim().to_string();
    if username.is_empty() {
        return SetupTemplate {
            error: Some("Username is required.".to_string()),
        }
        .into_response();
    }

    if form.password.len() < 8 {
        return SetupTemplate {
            error: Some("Password must be at least 8 characters.".to_string()),
        }
        .into_response();
    }

    if form.password != form.password_confirm {
        return SetupTemplate {
            error: Some("Passwords do not match.".to_string()),
        }
        .into_response();
    }

    let zt_token = form.zt_token.trim().to_string();
    if zt_token.is_empty() {
        return SetupTemplate {
            error: Some("ZeroTier auth token is required.".to_string()),
        }
        .into_response();
    }

    let password_hash = match hash_password(&form.password) {
        Ok(h) => h,
        Err(e) => {
            return SetupTemplate {
                error: Some(format!("Internal error: {}", e)),
            }
            .into_response();
        }
    };

    let zt_base_url =
        std::env::var("ZT_BASE_URL").unwrap_or_else(|_| "http://localhost:9993".to_string());

    let config = Config {
        username,
        password_hash,
        zt_token,
        zt_base_url,
        member_names: std::collections::HashMap::new(),
        rules_source: std::collections::HashMap::new(),
    };

    if let Err(e) = state.configure(config).await {
        return SetupTemplate {
            error: Some(format!("Failed to save configuration: {}", e)),
        }
        .into_response();
    }

    Redirect::to("/login").into_response()
}

// ---- Login ----

#[derive(askama::Template, askama_web::WebTemplate)]
#[template(path = "login.html")]
pub struct LoginTemplate {
    pub error: Option<String>,
}

#[derive(Deserialize)]
pub struct LoginForm {
    pub username: String,
    pub password: String,
}

/// GET /login
pub async fn login_page(
    State(state): State<AppState>,
    session: Session,
) -> Response {
    if !state.is_configured().await {
        return Redirect::to("/setup").into_response();
    }

    let is_authenticated = session
        .get::<bool>(SESSION_USER_KEY)
        .await
        .unwrap_or(None)
        .unwrap_or(false);

    if is_authenticated {
        return Redirect::to("/").into_response();
    }

    LoginTemplate { error: None }.into_response()
}

/// POST /login
pub async fn login_submit(
    session: Session,
    State(state): State<AppState>,
    Form(form): Form<LoginForm>,
) -> Response {
    let config = state.config.read().await;
    let config = match config.as_ref() {
        Some(c) => c,
        None => return Redirect::to("/setup").into_response(),
    };

    let username_match = form.username.trim() == config.username;
    let password_match = verify_password(&form.password, &config.password_hash);

    if username_match && password_match {
        session
            .insert(SESSION_USER_KEY, true)
            .await
            .unwrap_or_default();
        Redirect::to("/").into_response()
    } else {
        let tmpl = LoginTemplate {
            error: Some("Invalid username or password.".to_string()),
        };
        (StatusCode::UNAUTHORIZED, tmpl).into_response()
    }
}

/// GET /logout
pub async fn logout(session: Session) -> Redirect {
    session.flush().await.unwrap_or_default();
    Redirect::to("/login")
}
