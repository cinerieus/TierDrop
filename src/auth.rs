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

use crate::state::{AppState, Config, User};

const SESSION_USER_ID_KEY: &str = "user_id";
const SESSION_2FA_PENDING_KEY: &str = "2fa_pending";

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

// ---- Session Helpers ----

/// Get the current user ID from the session
pub async fn get_session_user_id(session: &Session) -> Option<u64> {
    session.get::<u64>(SESSION_USER_ID_KEY).await.ok().flatten()
}

/// Get the user ID pending 2FA verification
pub async fn get_2fa_pending_user_id(session: &Session) -> Option<u64> {
    session.get::<u64>(SESSION_2FA_PENDING_KEY).await.ok().flatten()
}

/// Get the current user from session + config
pub async fn get_current_user(session: &Session, state: &AppState) -> Option<User> {
    let user_id = get_session_user_id(session).await?;
    let config = state.config.read().await;
    config.as_ref()?.find_user_by_id(user_id).cloned()
}

/// Check if user is authenticated (has valid session)
pub async fn is_authenticated(session: &Session, state: &AppState) -> bool {
    get_current_user(session, state).await.is_some()
}

// ---- Middleware ----

/// Auth middleware — redirects to /setup if unconfigured, /login if unauthenticated
/// Also stores the current user in request extensions for route handlers
pub async fn auth_middleware(
    State(state): State<AppState>,
    session: Session,
    mut request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    if !state.is_configured().await {
        return Redirect::to("/setup").into_response();
    }

    if let Some(user) = get_current_user(&session, &state).await {
        // Store user in request extensions for easy access in handlers
        request.extensions_mut().insert(user);
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

    // Create the first admin user with ID 1
    let admin_user = User::new_admin(1, username, password_hash);

    let config = Config {
        username: None,
        password_hash: None,
        users: vec![admin_user],
        next_user_id: 2,
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

    if is_authenticated(&session, &state).await {
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

    let username = form.username.trim();

    // Find user by username
    if let Some(user) = config.find_user_by_username(username) {
        if verify_password(&form.password, &user.password_hash) {
            // Check if 2FA is enabled
            if user.totp_enabled && user.totp_secret.is_some() {
                // Store user ID in pending 2FA state
                session
                    .insert(SESSION_2FA_PENDING_KEY, user.id)
                    .await
                    .unwrap_or_default();
                return Redirect::to("/login/2fa").into_response();
            }

            // No 2FA - complete login directly
            session
                .insert(SESSION_USER_ID_KEY, user.id)
                .await
                .unwrap_or_default();
            return Redirect::to("/").into_response();
        }
    }

    let tmpl = LoginTemplate {
        error: Some("Invalid username or password.".to_string()),
    };
    (StatusCode::UNAUTHORIZED, tmpl).into_response()
}

/// GET /logout
pub async fn logout(session: Session) -> Redirect {
    session.flush().await.unwrap_or_default();
    Redirect::to("/login")
}

// ---- 2FA Verification ----

#[derive(askama::Template, askama_web::WebTemplate)]
#[template(path = "login_2fa.html")]
pub struct Login2faTemplate {
    pub error: Option<String>,
}

#[derive(Deserialize)]
pub struct Login2faForm {
    pub code: String,
}

/// GET /login/2fa
pub async fn login_2fa_page(
    State(state): State<AppState>,
    session: Session,
) -> Response {
    // If already authenticated, go to dashboard
    if is_authenticated(&session, &state).await {
        return Redirect::to("/").into_response();
    }

    // Must have pending 2FA
    if get_2fa_pending_user_id(&session).await.is_none() {
        return Redirect::to("/login").into_response();
    }

    Login2faTemplate { error: None }.into_response()
}

/// POST /login/2fa
pub async fn login_2fa_submit(
    session: Session,
    State(state): State<AppState>,
    Form(form): Form<Login2faForm>,
) -> Response {
    // Get pending user ID
    let pending_user_id = match get_2fa_pending_user_id(&session).await {
        Some(id) => id,
        None => return Redirect::to("/login").into_response(),
    };

    // Get user from config
    let config = state.config.read().await;
    let user = match config.as_ref().and_then(|c| c.find_user_by_id(pending_user_id)) {
        Some(u) => u.clone(),
        None => {
            // User no longer exists
            session.remove::<u64>(SESSION_2FA_PENDING_KEY).await.unwrap_or_default();
            return Redirect::to("/login").into_response();
        }
    };
    drop(config);

    // Get TOTP secret
    let secret = match &user.totp_secret {
        Some(s) => s,
        None => {
            // 2FA not properly configured
            session.remove::<u64>(SESSION_2FA_PENDING_KEY).await.unwrap_or_default();
            return Redirect::to("/login").into_response();
        }
    };

    // Verify TOTP code
    let code = form.code.trim().replace(" ", "");
    if verify_totp(&code, secret) {
        // Clear pending state
        session.remove::<u64>(SESSION_2FA_PENDING_KEY).await.unwrap_or_default();
        // Complete login
        session
            .insert(SESSION_USER_ID_KEY, user.id)
            .await
            .unwrap_or_default();
        return Redirect::to("/").into_response();
    }

    Login2faTemplate {
        error: Some("Invalid verification code.".to_string()),
    }
    .into_response()
}

/// Verify a TOTP code against a secret
pub fn verify_totp(code: &str, secret: &str) -> bool {
    use totp_rs::{Algorithm, TOTP, Secret};

    let secret = match Secret::Encoded(secret.to_string()).to_bytes() {
        Ok(s) => s,
        Err(_) => return false,
    };

    let totp = match TOTP::new(
        Algorithm::SHA1,
        6,
        1,
        30,
        secret,
        None,
        String::new(),
    ) {
        Ok(t) => t,
        Err(_) => return false,
    };

    totp.check_current(code).unwrap_or(false)
}
