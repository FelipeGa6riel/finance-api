use crate::auth::{AuthConfig, Claims, LoginRateLimiter, encode_token};
use crate::db::{SqlitePool, now_utc};
use crate::errors::{ApiError, ApiResult};
use crate::models::{ChangePasswordRequest, UpdateMeRequest, User, UserResponse};
use crate::schema::users;
use crate::services::ownership::load_active_user;
use actix_web::{HttpRequest, HttpResponse, get, post, put, web};
use diesel::prelude::*;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[post("/login")]
pub async fn login(
    req: HttpRequest,
    credentials: web::Json<LoginRequest>,
    pool: web::Data<SqlitePool>,
    auth_cfg: web::Data<AuthConfig>,
    limiter: web::Data<LoginRateLimiter>,
) -> ApiResult<HttpResponse> {
    let client_key = req
        .connection_info()
        .realip_remote_addr()
        .unwrap_or("unknown")
        .to_string();
    limiter.check_and_record(&client_key)?;

    let mut conn = pool.get()?;
    let email_clean = credentials.email.trim().to_lowercase();

    let user = users::table
        .filter(users::email.eq(&email_clean))
        .filter(users::deleted_at.is_null())
        .first::<User>(&mut conn)
        .optional()?;

    let Some(user) = user else {
        return Err(ApiError::Unauthorized("Credenciais inválidas".into()));
    };

    let hash_password = user.password.unwrap_or_default();
    let is_valid = bcrypt::verify(&credentials.password, &hash_password).unwrap_or(false);
    if !is_valid {
        return Err(ApiError::Unauthorized("Credenciais inválidas".into()));
    }

    let token = encode_token(&user.id, &auth_cfg.secret_key)?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "token": token })))
}

#[get("/me")]
pub async fn get_me(
    pool: web::Data<SqlitePool>,
    claims: web::ReqData<Claims>,
) -> ApiResult<HttpResponse> {
    let mut conn = pool.get()?;
    let user = load_active_user(&mut conn, &claims.sub)?;
    Ok(HttpResponse::Ok().json(UserResponse::from(user)))
}

fn apply_me_update(
    conn: &mut SqliteConnection,
    user_id: &str,
    payload: &UpdateMeRequest,
) -> ApiResult<UserResponse> {
    let existing = load_active_user(conn, user_id)?;
    let now = now_utc();

    let new_name = payload
        .name
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or(existing.name);
    let new_currency = payload
        .currency
        .as_ref()
        .map(|s| s.trim().to_uppercase())
        .filter(|s| !s.is_empty())
        .or(existing.currency);
    let new_cycle_day = match payload.cycle_start_day {
        Some(d) if (1..=28).contains(&d) => d,
        Some(_) => {
            return Err(ApiError::BadRequest(
                "cycle_start_day deve estar entre 1 e 28".into(),
            ));
        }
        None => existing.cycle_start_day,
    };
    let new_savings_pct = match payload.default_savings_percent {
        Some(p) if (0.0..=100.0).contains(&p) => p,
        Some(_) => {
            return Err(ApiError::BadRequest(
                "default_savings_percent deve estar entre 0 e 100".into(),
            ));
        }
        None => existing.default_savings_percent,
    };

    diesel::update(users::table.filter(users::id.eq(user_id)))
        .set((
            users::name.eq(new_name),
            users::currency.eq(new_currency),
            users::cycle_start_day.eq(new_cycle_day),
            users::default_savings_percent.eq(new_savings_pct),
            users::updated_at.eq(now),
        ))
        .execute(conn)?;

    let updated = load_active_user(conn, user_id)?;
    Ok(UserResponse::from(updated))
}

#[put("/me")]
pub async fn update_me(
    payload: web::Json<UpdateMeRequest>,
    pool: web::Data<SqlitePool>,
    claims: web::ReqData<Claims>,
) -> ApiResult<HttpResponse> {
    let mut conn = pool.get()?;
    let updated = apply_me_update(&mut conn, &claims.sub, &payload)?;
    Ok(HttpResponse::Ok().json(updated))
}

#[post("/me/password")]
pub async fn change_password(
    payload: web::Json<ChangePasswordRequest>,
    pool: web::Data<SqlitePool>,
    claims: web::ReqData<Claims>,
) -> ApiResult<HttpResponse> {
    if payload.new_password.len() < 6 {
        return Err(ApiError::BadRequest(
            "A nova senha deve ter pelo menos 6 caracteres".into(),
        ));
    }
    let mut conn = pool.get()?;
    let user = load_active_user(&mut conn, &claims.sub)?;
    let current_hash = user.password.unwrap_or_default();
    if !bcrypt::verify(&payload.current_password, &current_hash).unwrap_or(false) {
        return Err(ApiError::Unauthorized("Senha atual incorreta".into()));
    }

    let new_hash = bcrypt::hash(&payload.new_password, bcrypt::DEFAULT_COST)
        .map_err(|e| ApiError::Internal(format!("Falha ao gerar hash da senha: {e}")))?;

    diesel::update(users::table.filter(users::id.eq(&claims.sub)))
        .set((
            users::password.eq(Some(new_hash)),
            users::updated_at.eq(now_utc()),
        ))
        .execute(&mut conn)?;

    Ok(HttpResponse::Ok().json(serde_json::json!({ "updated": true })))
}

#[get("/users")]
pub async fn get_users(
    pool: web::Data<SqlitePool>,
    claims: web::ReqData<Claims>,
) -> ApiResult<HttpResponse> {
    let mut conn = pool.get()?;
    let user = load_active_user(&mut conn, &claims.sub)?;
    Ok(HttpResponse::Ok().json(vec![UserResponse::from(user)]))
}

#[get("/users/{id}")]
pub async fn get_user(
    user_id: web::Path<String>,
    pool: web::Data<SqlitePool>,
    claims: web::ReqData<Claims>,
) -> ApiResult<HttpResponse> {
    let target = user_id.into_inner();
    if target != claims.sub {
        return Err(ApiError::NotFound("Usuário não encontrado".into()));
    }
    let mut conn = pool.get()?;
    let user = load_active_user(&mut conn, &claims.sub)?;
    Ok(HttpResponse::Ok().json(UserResponse::from(user)))
}

#[post("/users/{id}")]
pub async fn update_user(
    user_id: web::Path<String>,
    payload: web::Json<UpdateMeRequest>,
    pool: web::Data<SqlitePool>,
    claims: web::ReqData<Claims>,
) -> ApiResult<HttpResponse> {
    let target = user_id.into_inner();
    if target != claims.sub {
        return Err(ApiError::NotFound("Usuário não encontrado".into()));
    }
    let mut conn = pool.get()?;
    let updated = apply_me_update(&mut conn, &claims.sub, &payload)?;
    Ok(HttpResponse::Ok().json(updated))
}
