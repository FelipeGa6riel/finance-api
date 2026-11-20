use crate::auth::Claims;
use crate::cycles::resolve_cycle_window;
use crate::db::{SqlitePool, now_utc};
use crate::errors::{ApiError, ApiResult};
use crate::models::{Account, AccountTransfer, CreateTransferRequest, normalize_transfer_kind};
use crate::schema::{account_transfers, accounts};
use crate::services::ownership::{
    hydrate_transfers, load_active_user, load_owned_account, load_owned_transfer,
    replace_transfer_tags, validate_owned_tags,
};
use actix_web::{HttpResponse, delete, get, post, web};
use diesel::prelude::*;
use serde::Deserialize;
use uuid::Uuid;

#[derive(Deserialize, Default)]
pub struct TransferFilters {
    pub cycle_key: Option<String>,
    pub transfer_kind: Option<String>,
}

#[get("/transfers")]
pub async fn get_transfers(
    query: web::Query<TransferFilters>,
    pool: web::Data<SqlitePool>,
    claims: web::ReqData<Claims>,
) -> ApiResult<HttpResponse> {
    let mut conn = pool.get()?;
    let user = load_active_user(&mut conn, &claims.sub)?;

    let mut q = account_transfers::table
        .filter(account_transfers::user_id.eq(&claims.sub))
        .filter(account_transfers::deleted_at.is_null())
        .into_boxed();

    if let Some(ref ck) = query.cycle_key {
        if !ck.trim().is_empty() {
            let (s_dt, e_dt, _, _) = resolve_cycle_window(&mut conn, &user, ck.trim())?;
            q = q
                .filter(account_transfers::date.ge(s_dt))
                .filter(account_transfers::date.le(e_dt));
        }
    }

    if let Some(ref kind) = query.transfer_kind {
        if !kind.trim().is_empty() {
            let norm = normalize_transfer_kind(kind).map_err(ApiError::BadRequest)?;
            q = q.filter(account_transfers::transfer_kind.eq(norm));
        }
    }

    let rows = q
        .order((account_transfers::date.desc(), account_transfers::created_at.desc()))
        .load::<AccountTransfer>(&mut conn)?;

    let hydrated = hydrate_transfers(&mut conn, rows)?;
    Ok(HttpResponse::Ok().json(hydrated))
}

#[post("/transfers")]
pub async fn create_transfer(
    payload: web::Json<CreateTransferRequest>,
    pool: web::Data<SqlitePool>,
    claims: web::ReqData<Claims>,
) -> ApiResult<HttpResponse> {
    if payload.amount <= 0 {
        return Err(ApiError::BadRequest(
            "Valor da transferência deve ser maior que zero".into(),
        ));
    }
    let kind = match &payload.transfer_kind {
        Some(k) => normalize_transfer_kind(k).map_err(ApiError::BadRequest)?,
        None => "general",
    };

    if kind != "invoice_payment" && payload.to_account_id.as_deref().unwrap_or("").trim().is_empty()
    {
        return Err(ApiError::BadRequest(
            "to_account_id é obrigatório para transferências".into(),
        ));
    }

    let mut conn = pool.get()?;
    let now = now_utc();
    let tr_date = payload.date.unwrap_or(now);
    let tag_ids = payload.tag_ids.clone().unwrap_or_default();

    let created = conn.transaction::<_, ApiError, _>(|conn| {
        let from_acc = load_owned_account(conn, &payload.from_account_id, &claims.sub)?;
        let to_acc = if let Some(ref to_id) = payload.to_account_id {
            if to_id == &from_acc.id {
                return Err(ApiError::BadRequest(
                    "Conta de origem e destino devem ser diferentes".into(),
                ));
            }
            Some(load_owned_account(conn, to_id, &claims.sub)?)
        } else {
            None
        };

        let _ = validate_owned_tags(conn, &tag_ids, &claims.sub)?;

        diesel::update(accounts::table.filter(accounts::id.eq(&from_acc.id)))
            .set((
                accounts::balance.eq(from_acc.balance - payload.amount),
                accounts::updated_at.eq(now),
            ))
            .execute(conn)?;

        if let Some(ref dest) = to_acc {
            diesel::update(accounts::table.filter(accounts::id.eq(&dest.id)))
                .set((
                    accounts::balance.eq(dest.balance + payload.amount),
                    accounts::updated_at.eq(now),
                ))
                .execute(conn)?;
        }

        let tr = AccountTransfer {
            id: Uuid::new_v4().to_string(),
            from_account_id: from_acc.id,
            to_account_id: to_acc.map(|a| a.id),
            amount: payload.amount,
            description: payload.description.clone(),
            date: tr_date,
            transfer_kind: kind.to_string(),
            user_id: claims.sub.clone(),
            deleted_at: None,
            created_at: now,
            updated_at: now,
        };

        diesel::insert_into(account_transfers::table)
            .values(&tr)
            .execute(conn)?;

        if !tag_ids.is_empty() {
            replace_transfer_tags(conn, &tr.id, &tag_ids)?;
        }

        Ok(tr)
    })?;

    let mut hydrated = hydrate_transfers(&mut conn, vec![created])?;
    Ok(HttpResponse::Created().json(hydrated.remove(0)))
}

#[delete("/transfers/{id}")]
pub async fn delete_transfer(
    tr_id: web::Path<String>,
    pool: web::Data<SqlitePool>,
    claims: web::ReqData<Claims>,
) -> ApiResult<HttpResponse> {
    let id = tr_id.into_inner();
    let mut conn = pool.get()?;
    let now = now_utc();

    conn.transaction::<_, ApiError, _>(|conn| {
        let tr = load_owned_transfer(conn, &id, &claims.sub)?;

        let from_acc = accounts::table
            .filter(accounts::id.eq(&tr.from_account_id))
            .filter(accounts::user_id.eq(&claims.sub))
            .first::<Account>(conn)?;
        diesel::update(accounts::table.filter(accounts::id.eq(&from_acc.id)))
            .set((
                accounts::balance.eq(from_acc.balance + tr.amount),
                accounts::updated_at.eq(now),
            ))
            .execute(conn)?;

        if let Some(ref to_id) = tr.to_account_id {
            let to_acc = accounts::table
                .filter(accounts::id.eq(to_id))
                .filter(accounts::user_id.eq(&claims.sub))
                .first::<Account>(conn)?;
            diesel::update(accounts::table.filter(accounts::id.eq(&to_acc.id)))
                .set((
                    accounts::balance.eq(to_acc.balance - tr.amount),
                    accounts::updated_at.eq(now),
                ))
                .execute(conn)?;
        }

        diesel::update(
            account_transfers::table
                .filter(account_transfers::id.eq(&id))
                .filter(account_transfers::user_id.eq(&claims.sub)),
        )
        .set((
            account_transfers::deleted_at.eq(Some(now)),
            account_transfers::updated_at.eq(now),
        ))
        .execute(conn)?;

        Ok(())
    })?;

    Ok(HttpResponse::NoContent().finish())
}
