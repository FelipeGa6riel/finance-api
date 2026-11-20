use crate::auth::Claims;
use crate::db::{SqlitePool, now_utc};
use crate::errors::{ApiError, ApiResult};
use crate::models::{Account, CreateAccountRequest, UpdateAccountRequest, normalize_account_type};
use crate::schema::accounts;
use crate::services::ownership::load_owned_account;
use actix_web::{HttpResponse, delete, get, post, put, web};
use diesel::prelude::*;
use uuid::Uuid;

#[get("/accounts")]
pub async fn get_accounts(
    pool: web::Data<SqlitePool>,
    claims: web::ReqData<Claims>,
) -> ApiResult<HttpResponse> {
    let mut conn = pool.get()?;
    let list = accounts::table
        .filter(accounts::user_id.eq(&claims.sub))
        .filter(accounts::deleted_at.is_null())
        .order(accounts::created_at.asc())
        .load::<Account>(&mut conn)?;
    Ok(HttpResponse::Ok().json(list))
}

#[get("/accounts/{id}")]
pub async fn get_account(
    account_id: web::Path<String>,
    pool: web::Data<SqlitePool>,
    claims: web::ReqData<Claims>,
) -> ApiResult<HttpResponse> {
    let mut conn = pool.get()?;
    let acc = load_owned_account(&mut conn, &account_id.into_inner(), &claims.sub)?;
    Ok(HttpResponse::Ok().json(acc))
}

#[post("/accounts")]
pub async fn create_account(
    payload: web::Json<CreateAccountRequest>,
    pool: web::Data<SqlitePool>,
    claims: web::ReqData<Claims>,
) -> ApiResult<HttpResponse> {
    let name = payload.name.trim().to_string();
    if name.is_empty() {
        return Err(ApiError::BadRequest("Nome da conta é obrigatório".into()));
    }
    let bank_name = payload.bank_name.trim().to_string();
    let acc_type = match &payload.account_type {
        Some(t) => normalize_account_type(t).map_err(ApiError::BadRequest)?,
        None => "checking",
    };

    let mut conn = pool.get()?;
    let now = now_utc();
    let new_acc = Account {
        id: Uuid::new_v4().to_string(),
        name,
        icon: payload.icon.clone(),
        balance: payload.balance.unwrap_or(0),
        bank_name,
        account_type: acc_type.to_string(),
        user_id: claims.sub.clone(),
        deleted_at: None,
        created_at: now,
        updated_at: now,
    };

    diesel::insert_into(accounts::table)
        .values(&new_acc)
        .execute(&mut conn)?;

    Ok(HttpResponse::Created().json(new_acc))
}

#[put("/accounts/{id}")]
pub async fn update_account(
    account_id: web::Path<String>,
    payload: web::Json<UpdateAccountRequest>,
    pool: web::Data<SqlitePool>,
    claims: web::ReqData<Claims>,
) -> ApiResult<HttpResponse> {
    let id = account_id.into_inner();
    let mut conn = pool.get()?;
    let existing = load_owned_account(&mut conn, &id, &claims.sub)?;

    let new_name = match &payload.name {
        Some(n) if !n.trim().is_empty() => n.trim().to_string(),
        Some(_) => return Err(ApiError::BadRequest("Nome da conta não pode ser vazio".into())),
        None => existing.name,
    };
    let new_icon = match &payload.icon {
        Some(v) => v.clone(),
        None => existing.icon,
    };
    let new_bank = payload
        .bank_name
        .as_ref()
        .map(|s| s.trim().to_string())
        .unwrap_or(existing.bank_name);
    let new_type = match &payload.account_type {
        Some(t) => normalize_account_type(t).map_err(ApiError::BadRequest)?.to_string(),
        None => existing.account_type,
    };
    let new_balance = payload.balance.unwrap_or(existing.balance);

    diesel::update(
        accounts::table.filter(accounts::id.eq(&id).and(accounts::user_id.eq(&claims.sub))),
    )
    .set((
        accounts::name.eq(new_name),
        accounts::icon.eq(new_icon),
        accounts::bank_name.eq(new_bank),
        accounts::account_type.eq(new_type),
        accounts::balance.eq(new_balance),
        accounts::updated_at.eq(now_utc()),
    ))
    .execute(&mut conn)?;

    let updated = load_owned_account(&mut conn, &id, &claims.sub)?;
    Ok(HttpResponse::Ok().json(updated))
}

#[delete("/accounts/{id}")]
pub async fn delete_account(
    account_id: web::Path<String>,
    pool: web::Data<SqlitePool>,
    claims: web::ReqData<Claims>,
) -> ApiResult<HttpResponse> {
    let id = account_id.into_inner();
    let mut conn = pool.get()?;
    let _existing = load_owned_account(&mut conn, &id, &claims.sub)?;
    let now = now_utc();

    diesel::update(
        accounts::table.filter(accounts::id.eq(&id).and(accounts::user_id.eq(&claims.sub))),
    )
    .set((accounts::deleted_at.eq(Some(now)), accounts::updated_at.eq(now)))
    .execute(&mut conn)?;

    Ok(HttpResponse::NoContent().finish())
}
