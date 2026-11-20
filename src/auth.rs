use crate::db::SqlitePool;
use crate::errors::ApiError;
use crate::models::User;
use actix_web::{HttpMessage, dev::ServiceRequest, web};
use actix_web_httpauth::extractors::bearer::BearerAuth;
use diesel::prelude::*;
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Claims {
    pub sub: String,
    pub exp: usize,
}

#[derive(Clone, Debug)]
pub struct AuthConfig {
    pub secret_key: String,
}

pub fn get_secret_key() -> String {
    let val = std::env::var("SECRET_KEY").expect("SECRET_KEY must be set");
    if val.trim().is_empty() {
        panic!("SECRET_KEY must not be empty");
    }
    val
}

pub fn encode_token(user_id: &str, secret_key: &str) -> Result<String, ApiError> {
    let expiration = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_err(|_| ApiError::Internal("Relógio do sistema inválido".into()))?
        .as_secs()
        + 24 * 3600;

    let claims = Claims {
        sub: user_id.to_string(),
        exp: expiration as usize,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret_key.as_bytes()),
    )
    .map_err(|e| ApiError::Internal(format!("Erro ao gerar token: {e}")))
}

pub struct LoginRateLimiter {
    attempts: Mutex<HashMap<String, Vec<Instant>>>,
    max_attempts: usize,
    window: Duration,
}

impl LoginRateLimiter {
    pub fn new() -> Self {
        Self::with_limit(20, 300)
    }

    pub fn with_limit(max_attempts: usize, window_secs: u64) -> Self {
        Self {
            attempts: Mutex::new(HashMap::new()),
            max_attempts,
            window: Duration::from_secs(window_secs),
        }
    }

    pub fn check_and_record(&self, key: &str) -> Result<(), ApiError> {
        let mut map = self
            .attempts
            .lock()
            .map_err(|_| ApiError::Internal("Erro interno no rate limiter".into()))?;
        let now = Instant::now();
        let entry = map.entry(key.to_string()).or_default();
        entry.retain(|t| now.duration_since(*t) < self.window);
        if entry.len() >= self.max_attempts {
            return Err(ApiError::TooManyRequests(
                "Muitas tentativas de login. Aguarde alguns minutos.".into(),
            ));
        }
        entry.push(now);
        Ok(())
    }
}

impl Default for LoginRateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

pub async fn validate_user_credentials(
    req: ServiceRequest,
    credentials: BearerAuth,
) -> Result<ServiceRequest, (actix_web::Error, ServiceRequest)> {
    let secret_key = match req.app_data::<web::Data<AuthConfig>>() {
        Some(cfg) => cfg.secret_key.clone(),
        None => {
            return Err((
                actix_web::error::ErrorInternalServerError("AuthConfig not configured"),
                req,
            ));
        }
    };

    let token_data = match decode::<Claims>(
        credentials.token(),
        &DecodingKey::from_secret(secret_key.as_bytes()),
        &Validation::default(),
    ) {
        Ok(data) => data,
        Err(_) => {
            return Err((
                actix_web::error::ErrorUnauthorized("Invalid or expired token"),
                req,
            ));
        }
    };

    let pool = match req.app_data::<web::Data<SqlitePool>>() {
        Some(p) => p,
        None => {
            return Err((
                actix_web::error::ErrorInternalServerError("Database pool not configured"),
                req,
            ));
        }
    };

    let mut conn = match pool.get() {
        Ok(c) => c,
        Err(_) => {
            return Err((
                actix_web::error::ErrorInternalServerError("Failed to get DB connection"),
                req,
            ));
        }
    };

    use crate::schema::users::dsl::{deleted_at, id, users};
    let active_user = users
        .filter(id.eq(&token_data.claims.sub).and(deleted_at.is_null()))
        .first::<User>(&mut conn)
        .optional();

    match active_user {
        Ok(Some(_)) => {
            req.extensions_mut().insert(token_data.claims);
            Ok(req)
        }
        Ok(None) => Err((
            actix_web::error::ErrorUnauthorized("User inactive or not found"),
            req,
        )),
        Err(_) => Err((
            actix_web::error::ErrorInternalServerError("Database error validating user"),
            req,
        )),
    }
}
