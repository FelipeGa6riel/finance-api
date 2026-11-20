use crate::auth::Claims;
use crate::cycles::{
    compute_cycle_summary, compute_overview, cycle_key_for_date, list_user_cycles,
    resolve_cycle_window,
};
use crate::db::{SqlitePool, now_utc};
use crate::errors::{ApiError, ApiResult};
use crate::models::{CycleConfig, UpsertCycleConfigRequest, format_ymd};
use crate::schema::cycle_configs;
use crate::services::ownership::load_active_user;
use actix_web::{HttpResponse, get, put, web};
use diesel::prelude::*;
use serde::Deserialize;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct SummaryQuery {
    pub cycle: Option<String>,
}

#[get("/cycles")]
pub async fn get_cycles(
    pool: web::Data<SqlitePool>,
    claims: web::ReqData<Claims>,
) -> ApiResult<HttpResponse> {
    let mut conn = pool.get()?;
    let user = load_active_user(&mut conn, &claims.sub)?;
    let list = list_user_cycles(&mut conn, &user)?;
    Ok(HttpResponse::Ok().json(list))
}

#[get("/cycles/summary")]
pub async fn get_cycle_summary(
    query: web::Query<SummaryQuery>,
    pool: web::Data<SqlitePool>,
    claims: web::ReqData<Claims>,
) -> ApiResult<HttpResponse> {
    let mut conn = pool.get()?;
    let user = load_active_user(&mut conn, &claims.sub)?;
    let cycle_key = query
        .cycle
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| cycle_key_for_date(now_utc().date(), user.cycle_start_day));

    let summary = compute_cycle_summary(&mut conn, &user, &cycle_key, true, true)?;
    Ok(HttpResponse::Ok().json(summary))
}

#[put("/cycles/{cycle_key}/config")]
pub async fn upsert_cycle_config(
    path: web::Path<String>,
    payload: web::Json<UpsertCycleConfigRequest>,
    pool: web::Data<SqlitePool>,
    claims: web::ReqData<Claims>,
) -> ApiResult<HttpResponse> {
    let cycle_key = path.into_inner().trim().to_string();
    let mode = payload.savings_mode.trim().to_lowercase();
    if mode != "percent" && mode != "fixed" {
        return Err(ApiError::BadRequest(
            "savings_mode deve ser 'percent' ou 'fixed'".into(),
        ));
    }

    let mut conn = pool.get()?;
    let user = load_active_user(&mut conn, &claims.sub)?;
    let (default_start, default_end, _, existing) =
        resolve_cycle_window(&mut conn, &user, &cycle_key)?;
    let now = now_utc();

    if let Some(existing_cfg) = existing {
        let new_start = payload
            .start_date
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(existing_cfg.start_date);
        let new_end = payload
            .end_date
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(existing_cfg.end_date);
        let new_label = match &payload.label {
            Some(v) => v.clone(),
            None => existing_cfg.label,
        };
        let new_pct = match &payload.savings_percent {
            Some(v) => *v,
            None => existing_cfg.savings_percent,
        };
        let new_fixed = match &payload.savings_fixed_amount {
            Some(v) => *v,
            None => existing_cfg.savings_fixed_amount,
        };
        let new_notes = match &payload.notes {
            Some(v) => v.clone(),
            None => existing_cfg.notes,
        };

        diesel::update(
            cycle_configs::table
                .filter(cycle_configs::id.eq(&existing_cfg.id))
                .filter(cycle_configs::user_id.eq(&claims.sub)),
        )
        .set((
            cycle_configs::start_date.eq(new_start),
            cycle_configs::end_date.eq(new_end),
            cycle_configs::label.eq(new_label),
            cycle_configs::savings_mode.eq(mode),
            cycle_configs::savings_percent.eq(new_pct),
            cycle_configs::savings_fixed_amount.eq(new_fixed),
            cycle_configs::notes.eq(new_notes),
            cycle_configs::updated_at.eq(now),
        ))
        .execute(&mut conn)?;

        let updated = cycle_configs::table
            .filter(cycle_configs::id.eq(&existing_cfg.id))
            .first::<CycleConfig>(&mut conn)?;
        return Ok(HttpResponse::Ok().json(updated));
    }

    let cfg = CycleConfig {
        id: Uuid::new_v4().to_string(),
        user_id: claims.sub.clone(),
        cycle_key,
        start_date: payload
            .start_date
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| format_ymd(default_start.date())),
        end_date: payload
            .end_date
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| format_ymd(default_end.date())),
        label: payload.label.clone().flatten(),
        savings_mode: mode,
        savings_percent: payload.savings_percent.flatten(),
        savings_fixed_amount: payload.savings_fixed_amount.flatten(),
        notes: payload.notes.clone().flatten(),
        created_at: now,
        updated_at: now,
    };

    diesel::insert_into(cycle_configs::table)
        .values(&cfg)
        .execute(&mut conn)?;

    Ok(HttpResponse::Ok().json(cfg))
}

#[get("/overview")]
pub async fn get_overview(
    pool: web::Data<SqlitePool>,
    claims: web::ReqData<Claims>,
) -> ApiResult<HttpResponse> {
    let mut conn = pool.get()?;
    let user = load_active_user(&mut conn, &claims.sub)?;
    let overview = compute_overview(&mut conn, &user)?;
    Ok(HttpResponse::Ok().json(overview))
}
