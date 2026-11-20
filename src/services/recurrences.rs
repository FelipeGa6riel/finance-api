use crate::auth::Claims;
use crate::cycles::{ensure_cycle_recurrences, resolve_cycle_window};
use crate::db::{SqlitePool, now_utc};
use crate::errors::{ApiError, ApiResult};
use crate::models::{
    CreateRecurringRuleRequest, GenerateRecurringRequest, RecurringRule, Type,
    UpdateRecurringRuleRequest, normalize_payment_method,
};
use crate::schema::recurring_rules;
use crate::services::ownership::{
    load_active_user, load_owned_account, load_owned_category, load_owned_recurring,
};
use actix_web::{HttpResponse, delete, get, post, put, web};
use diesel::prelude::*;
use uuid::Uuid;

#[get("/recurrences")]
pub async fn get_recurrences(
    pool: web::Data<SqlitePool>,
    claims: web::ReqData<Claims>,
) -> ApiResult<HttpResponse> {
    let mut conn = pool.get()?;
    let list = recurring_rules::table
        .filter(recurring_rules::user_id.eq(&claims.sub))
        .filter(recurring_rules::deleted_at.is_null())
        .order(recurring_rules::day_of_month.asc())
        .load::<RecurringRule>(&mut conn)?;
    Ok(HttpResponse::Ok().json(list))
}

#[post("/recurrences")]
pub async fn create_recurrence(
    payload: web::Json<CreateRecurringRuleRequest>,
    pool: web::Data<SqlitePool>,
    claims: web::ReqData<Claims>,
) -> ApiResult<HttpResponse> {
    let desc = payload.description.trim().to_string();
    if desc.is_empty() {
        return Err(ApiError::BadRequest("Descrição é obrigatória".into()));
    }
    if payload.default_amount < 0 {
        return Err(ApiError::BadRequest("default_amount não pode ser negativo".into()));
    }
    if !(1..=31).contains(&payload.day_of_month) {
        return Err(ApiError::BadRequest("day_of_month deve estar entre 1 e 31".into()));
    }
    let pm = match &payload.payment_method {
        Some(p) => normalize_payment_method(p).map_err(ApiError::BadRequest)?,
        None => "pix",
    };

    let mut conn = pool.get()?;
    let _acc = load_owned_account(&mut conn, &payload.account_id, &claims.sub)?;
    if let Some(ref cid) = payload.category_id {
        let _ = load_owned_category(&mut conn, cid, &claims.sub)?;
    }

    let now = now_utc();
    let rule = RecurringRule {
        id: Uuid::new_v4().to_string(),
        user_id: claims.sub.clone(),
        description: desc,
        default_amount: payload.default_amount,
        type_: payload.type_.unwrap_or(Type::Expense),
        payment_method: pm.to_string(),
        account_id: payload.account_id.clone(),
        category_id: payload.category_id.clone(),
        day_of_month: payload.day_of_month,
        is_variable_amount: payload.is_variable_amount.unwrap_or(false),
        is_fixed: payload.is_fixed.unwrap_or(true),
        active: payload.active.unwrap_or(true),
        deleted_at: None,
        created_at: now,
        updated_at: now,
    };

    diesel::insert_into(recurring_rules::table)
        .values(&rule)
        .execute(&mut conn)?;

    Ok(HttpResponse::Created().json(rule))
}

#[put("/recurrences/{id}")]
pub async fn update_recurrence(
    rule_id: web::Path<String>,
    payload: web::Json<UpdateRecurringRuleRequest>,
    pool: web::Data<SqlitePool>,
    claims: web::ReqData<Claims>,
) -> ApiResult<HttpResponse> {
    let id = rule_id.into_inner();
    let mut conn = pool.get()?;
    let existing = load_owned_recurring(&mut conn, &id, &claims.sub)?;

    let new_desc = match &payload.description {
        Some(d) if !d.trim().is_empty() => d.trim().to_string(),
        Some(_) => return Err(ApiError::BadRequest("Descrição não pode ser vazia".into())),
        None => existing.description,
    };
    let new_amount = payload.default_amount.unwrap_or(existing.default_amount);
    if new_amount < 0 {
        return Err(ApiError::BadRequest("default_amount não pode ser negativo".into()));
    }
    let new_day = payload.day_of_month.unwrap_or(existing.day_of_month);
    if !(1..=31).contains(&new_day) {
        return Err(ApiError::BadRequest("day_of_month deve estar entre 1 e 31".into()));
    }
    let new_pm = match &payload.payment_method {
        Some(p) => normalize_payment_method(p).map_err(ApiError::BadRequest)?.to_string(),
        None => existing.payment_method,
    };
    let new_acc = match &payload.account_id {
        Some(aid) => {
            let _ = load_owned_account(&mut conn, aid, &claims.sub)?;
            aid.clone()
        }
        None => existing.account_id,
    };
    let new_cat = match &payload.category_id {
        Some(Some(cid)) => {
            let _ = load_owned_category(&mut conn, cid, &claims.sub)?;
            Some(cid.clone())
        }
        Some(None) => None,
        None => existing.category_id,
    };

    diesel::update(
        recurring_rules::table
            .filter(recurring_rules::id.eq(&id))
            .filter(recurring_rules::user_id.eq(&claims.sub)),
    )
    .set((
        recurring_rules::description.eq(new_desc),
        recurring_rules::default_amount.eq(new_amount),
        recurring_rules::type_.eq(payload.type_.unwrap_or(existing.type_)),
        recurring_rules::payment_method.eq(new_pm),
        recurring_rules::account_id.eq(new_acc),
        recurring_rules::category_id.eq(new_cat),
        recurring_rules::day_of_month.eq(new_day),
        recurring_rules::is_variable_amount.eq(payload.is_variable_amount.unwrap_or(existing.is_variable_amount)),
        recurring_rules::is_fixed.eq(payload.is_fixed.unwrap_or(existing.is_fixed)),
        recurring_rules::active.eq(payload.active.unwrap_or(existing.active)),
        recurring_rules::updated_at.eq(now_utc()),
    ))
    .execute(&mut conn)?;

    let updated = load_owned_recurring(&mut conn, &id, &claims.sub)?;
    Ok(HttpResponse::Ok().json(updated))
}

#[delete("/recurrences/{id}")]
pub async fn delete_recurrence(
    rule_id: web::Path<String>,
    pool: web::Data<SqlitePool>,
    claims: web::ReqData<Claims>,
) -> ApiResult<HttpResponse> {
    let id = rule_id.into_inner();
    let mut conn = pool.get()?;
    let _existing = load_owned_recurring(&mut conn, &id, &claims.sub)?;
    let now = now_utc();

    diesel::update(
        recurring_rules::table
            .filter(recurring_rules::id.eq(&id))
            .filter(recurring_rules::user_id.eq(&claims.sub)),
    )
    .set((
        recurring_rules::active.eq(false),
        recurring_rules::deleted_at.eq(Some(now)),
        recurring_rules::updated_at.eq(now),
    ))
    .execute(&mut conn)?;

    Ok(HttpResponse::NoContent().finish())
}

#[post("/recurrences/generate")]
pub async fn generate_recurrences(
    payload: web::Json<GenerateRecurringRequest>,
    pool: web::Data<SqlitePool>,
    claims: web::ReqData<Claims>,
) -> ApiResult<HttpResponse> {
    let mut conn = pool.get()?;
    let user = load_active_user(&mut conn, &claims.sub)?;
    let (start_dt, end_dt, _, _) = resolve_cycle_window(&mut conn, &user, payload.cycle_key.trim())?;
    let created_count = ensure_cycle_recurrences(&mut conn, &user.id, start_dt, end_dt)?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "created_count": created_count })))
}
