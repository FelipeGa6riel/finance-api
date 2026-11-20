use crate::auth::Claims;
use crate::cycles::{last_day_of_month, resolve_cycle_window};
use crate::db::{SqlitePool, now_utc};
use crate::errors::{ApiError, ApiResult};
use crate::models::{
    Account, AccountTransfer, CreateTransactionRequest, PayCreditInvoiceRequest, Transaction,
    TransactionUpdateChangeset, Type, UpdateTransactionRequest, normalize_payment_method,
    parse_flexible_datetime, parse_ymd, transaction_balance_delta,
};
use crate::schema::{account_transfers, accounts, transaction_tags, transactions};
use crate::services::ownership::{
    hydrate_transactions, hydrate_transfers, load_active_user, load_owned_account,
    load_owned_category, load_owned_transaction, replace_transaction_tags, validate_owned_tags,
};
use actix_web::{HttpResponse, delete, get, post, put, web};
use diesel::prelude::*;
use serde::Deserialize;
use time::{Date, Month, PrimitiveDateTime, Time};
use uuid::Uuid;

#[derive(Deserialize, Default)]
pub struct TransactionFilters {
    pub cycle_key: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub account_id: Option<String>,
    pub category_id: Option<String>,
    pub payment_method: Option<String>,
    #[serde(rename = "type")]
    pub type_: Option<String>,
    pub tag_id: Option<String>,
    pub search: Option<String>,
    pub needs_confirmation: Option<bool>,
    pub only_open_credit: Option<bool>,
}

fn add_months(dt: PrimitiveDateTime, months: i32) -> PrimitiveDateTime {
    let mut y = dt.year();
    let mut m_idx = (dt.month() as i32) - 1 + months;
    y += m_idx.div_euclid(12);
    m_idx = m_idx.rem_euclid(12);
    let month = Month::try_from((m_idx + 1) as u8).expect("valid month");
    let day = dt.day().min(last_day_of_month(y, month));
    let d = Date::from_calendar_date(y, month, day).expect("valid date");
    PrimitiveDateTime::new(d, dt.time())
}

#[get("/transactions")]
pub async fn get_transactions(
    query: web::Query<TransactionFilters>,
    pool: web::Data<SqlitePool>,
    claims: web::ReqData<Claims>,
) -> ApiResult<HttpResponse> {
    let mut conn = pool.get()?;
    let user = load_active_user(&mut conn, &claims.sub)?;

    let mut q = transactions::table
        .filter(transactions::user_id.eq(&claims.sub))
        .filter(transactions::deleted_at.is_null())
        .into_boxed();

    if let Some(ref ck) = query.cycle_key {
        if !ck.trim().is_empty() {
            let (s_dt, e_dt, _, _) = resolve_cycle_window(&mut conn, &user, ck.trim())?;
            q = q.filter(transactions::date.ge(s_dt)).filter(transactions::date.le(e_dt));
        }
    } else {
        if let Some(ref f) = query.from {
            if !f.trim().is_empty() {
                let f_dt = parse_flexible_datetime(f).map_err(ApiError::BadRequest)?;
                q = q.filter(transactions::date.ge(f_dt));
            }
        }
        if let Some(ref t) = query.to {
            let trimmed = t.trim();
            if !trimmed.is_empty() {
                let t_dt = if trimmed.len() == 10 {
                    let d = parse_ymd(trimmed).map_err(ApiError::BadRequest)?;
                    PrimitiveDateTime::new(d, Time::from_hms(23, 59, 59).expect("valid"))
                } else {
                    parse_flexible_datetime(trimmed).map_err(ApiError::BadRequest)?
                };
                q = q.filter(transactions::date.le(t_dt));
            }
        }
    }

    if let Some(ref acc_id) = query.account_id {
        if !acc_id.trim().is_empty() {
            q = q.filter(transactions::account_id.eq(acc_id.trim()));
        }
    }
    if let Some(ref cat_id) = query.category_id {
        if !cat_id.trim().is_empty() {
            q = q.filter(transactions::category_id.eq(cat_id.trim()));
        }
    }
    if let Some(ref pm) = query.payment_method {
        if !pm.trim().is_empty() {
            let norm = normalize_payment_method(pm).map_err(ApiError::BadRequest)?;
            q = q.filter(transactions::payment_method.eq(norm));
        }
    }
    if let Some(ref tp) = query.type_ {
        match tp.trim().to_lowercase().as_str() {
            "income" => q = q.filter(transactions::type_.eq("income")),
            "expense" => q = q.filter(transactions::type_.eq("expense")),
            "transfer" => q = q.filter(transactions::type_.eq("transfer")),
            "" => {}
            other => return Err(ApiError::BadRequest(format!("Tipo inválido: '{other}'"))),
        }
    }
    if let Some(nc) = query.needs_confirmation {
        q = q.filter(transactions::needs_confirmation.eq(nc));
    }
    if query.only_open_credit == Some(true) {
        q = q
            .filter(transactions::payment_method.eq("credit"))
            .filter(transactions::is_informational.eq(false))
            .filter(transactions::credit_paid.eq(false));
    }
    if let Some(ref tag_id) = query.tag_id {
        if !tag_id.trim().is_empty() {
            let matching_tx_ids: Vec<String> = transaction_tags::table
                .filter(transaction_tags::tag_id.eq(tag_id.trim()))
                .select(transaction_tags::transaction_id)
                .load(&mut conn)?;
            q = q.filter(transactions::id.eq_any(matching_tx_ids));
        }
    }

    let mut rows = q
        .order((transactions::date.desc(), transactions::created_at.desc()))
        .load::<Transaction>(&mut conn)?;

    if let Some(ref search) = query.search {
        let needle = search.trim().to_lowercase();
        if !needle.is_empty() {
            rows.retain(|t| {
                t.description
                    .as_deref()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains(&needle)
            });
        }
    }

    let hydrated = hydrate_transactions(&mut conn, rows)?;
    Ok(HttpResponse::Ok().json(hydrated))
}

#[get("/transactions/{id}")]
pub async fn get_transaction(
    tx_id: web::Path<String>,
    pool: web::Data<SqlitePool>,
    claims: web::ReqData<Claims>,
) -> ApiResult<HttpResponse> {
    let mut conn = pool.get()?;
    let tx = load_owned_transaction(&mut conn, &tx_id.into_inner(), &claims.sub)?;
    let mut list = hydrate_transactions(&mut conn, vec![tx])?;
    Ok(HttpResponse::Ok().json(list.pop()))
}

#[post("/transactions")]
pub async fn create_transaction(
    payload: web::Json<CreateTransactionRequest>,
    pool: web::Data<SqlitePool>,
    claims: web::ReqData<Claims>,
) -> ApiResult<HttpResponse> {
    if payload.amount < 0 {
        return Err(ApiError::BadRequest("Valor não pode ser negativo".into()));
    }
    let payment_method = match &payload.payment_method {
        Some(pm) => normalize_payment_method(pm).map_err(ApiError::BadRequest)?,
        None => "pix",
    };

    let mut is_informational = payload.is_informational.unwrap_or(false);
    if payload.amount == 0 {
        if payment_method == "credit" || is_informational {
            is_informational = true;
        } else {
            return Err(ApiError::BadRequest(
                "Valor deve ser maior que zero (ou marque como informativo no crédito)".into(),
            ));
        }
    }

    let installments = payload.installment_total.unwrap_or(1);
    if installments < 1 || installments > 60 {
        return Err(ApiError::BadRequest(
            "installment_total deve estar entre 1 e 60".into(),
        ));
    }
    if installments > 1 && (payload.type_ != Type::Expense || payment_method != "credit" || is_informational) {
        return Err(ApiError::BadRequest(
            "Parcelamento só é permitido em despesas no crédito não informativas".into(),
        ));
    }

    let mut conn = pool.get()?;
    let now = now_utc();
    let tx_date = payload.date.unwrap_or(now);
    let tag_ids = payload.tag_ids.clone().unwrap_or_default();

    let created_rows = conn.transaction::<_, ApiError, _>(|conn| {
        let account = load_owned_account(conn, &payload.account_id, &claims.sub)?;
        if let Some(ref cat_id) = payload.category_id {
            let _ = load_owned_category(conn, cat_id, &claims.sub)?;
        }
        let _ = validate_owned_tags(conn, &tag_ids, &claims.sub)?;

        let card_name = payload
            .card_name
            .as_ref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let mut inserted = Vec::with_capacity(installments as usize);
        let group_id = if installments > 1 {
            Some(Uuid::new_v4().to_string())
        } else {
            None
        };

        let base_amount = payload.amount / (installments as i64);
        let remainder = payload.amount % (installments as i64);

        let mut total_delta: i64 = 0;

        for idx in 1..=installments {
            let inst_amount = if idx == 1 {
                base_amount + remainder
            } else {
                base_amount
            };
            let inst_date = if installments > 1 {
                add_months(tx_date, idx - 1)
            } else {
                tx_date
            };
            let inst_desc = match (&payload.description, installments > 1) {
                (Some(d), true) => Some(format!("{} ({}/{})", d.trim(), idx, installments)),
                (None, true) => Some(format!("Parcela {idx}/{installments}")),
                (other, false) => other.clone(),
            };

            let row = Transaction {
                id: Uuid::new_v4().to_string(),
                amount: inst_amount,
                description: inst_desc,
                date: inst_date,
                type_: payload.type_,
                payment_method: payment_method.to_string(),
                card_name: card_name.clone(),
                account_id: account.id.clone(),
                transfer_id: None,
                category_id: payload.category_id.clone(),
                is_fixed: payload.is_fixed,
                is_informational,
                credit_paid: false,
                installment_group_id: group_id.clone(),
                installment_number: if installments > 1 { Some(idx) } else { None },
                installment_total: if installments > 1 { Some(installments) } else { None },
                recurring_rule_id: None,
                needs_confirmation: payload.needs_confirmation.unwrap_or(false),
                user_id: claims.sub.clone(),
                deleted_at: None,
                created_at: now,
                updated_at: now,
            };

            diesel::insert_into(transactions::table)
                .values(&row)
                .execute(conn)?;
            if !tag_ids.is_empty() {
                replace_transaction_tags(conn, &row.id, &tag_ids)?;
            }

            total_delta += transaction_balance_delta(
                &row.type_,
                &row.payment_method,
                row.is_informational,
                row.amount,
            );
            inserted.push(row);
        }

        if total_delta != 0 {
            diesel::update(accounts::table.filter(accounts::id.eq(&account.id)))
                .set((
                    accounts::balance.eq(account.balance + total_delta),
                    accounts::updated_at.eq(now),
                ))
                .execute(conn)?;
        }

        Ok(inserted)
    })?;

    let mut hydrated = hydrate_transactions(&mut conn, created_rows)?;
    Ok(HttpResponse::Created().json(hydrated.remove(0)))
}

#[put("/transactions/{id}")]
pub async fn update_transaction(
    tx_id: web::Path<String>,
    payload: web::Json<UpdateTransactionRequest>,
    pool: web::Data<SqlitePool>,
    claims: web::ReqData<Claims>,
) -> ApiResult<HttpResponse> {
    let id = tx_id.into_inner();
    let mut conn = pool.get()?;
    let now = now_utc();

    let updated_row = conn.transaction::<_, ApiError, _>(|conn| {
        let old = load_owned_transaction(conn, &id, &claims.sub)?;

        let new_amount = payload.amount.unwrap_or(old.amount);
        if new_amount < 0 {
            return Err(ApiError::BadRequest("Valor não pode ser negativo".into()));
        }
        let new_type = payload.type_.unwrap_or(old.type_);
        let new_pm = match &payload.payment_method {
            Some(pm) => normalize_payment_method(pm)
                .map_err(ApiError::BadRequest)?
                .to_string(),
            None => old.payment_method.clone(),
        };
        let mut new_info = payload.is_informational.unwrap_or(old.is_informational);
        if new_amount == 0 {
            if new_pm == "credit" || new_info {
                new_info = true;
            } else {
                return Err(ApiError::BadRequest("Valor deve ser maior que zero".into()));
            }
        }

        let new_acc_id = payload
            .account_id
            .clone()
            .unwrap_or_else(|| old.account_id.clone());
        let new_acc = load_owned_account(conn, &new_acc_id, &claims.sub)?;

        let new_cat_id = match &payload.category_id {
            Some(Some(cid)) => {
                let _ = load_owned_category(conn, cid, &claims.sub)?;
                Some(cid.clone())
            }
            Some(None) => None,
            None => old.category_id.clone(),
        };

        if let Some(ref tids) = payload.tag_ids {
            let _ = validate_owned_tags(conn, tids, &claims.sub)?;
        }

        let new_desc = match &payload.description {
            Some(v) => v.clone(),
            None => old.description.clone(),
        };
        let new_card = match &payload.card_name {
            Some(Some(s)) => {
                let t = s.trim().to_string();
                if t.is_empty() { None } else { Some(t) }
            }
            Some(None) => None,
            None => old.card_name.clone(),
        };
        let new_fixed = match payload.is_fixed {
            Some(v) => v,
            None => old.is_fixed,
        };
        let new_date = payload.date.unwrap_or(old.date);
        let new_credit_paid = payload.credit_paid.unwrap_or(old.credit_paid);
        let new_needs_conf = payload.needs_confirmation.unwrap_or(old.needs_confirmation);

        let old_delta = transaction_balance_delta(
            &old.type_,
            &old.payment_method,
            old.is_informational,
            old.amount,
        );
        let new_delta = transaction_balance_delta(&new_type, &new_pm, new_info, new_amount);

        if new_acc.id == old.account_id {
            let diff = new_delta - old_delta;
            if diff != 0 {
                diesel::update(accounts::table.filter(accounts::id.eq(&new_acc.id)))
                    .set((
                        accounts::balance.eq(new_acc.balance + diff),
                        accounts::updated_at.eq(now),
                    ))
                    .execute(conn)?;
            }
        } else {
            if old_delta != 0 {
                let old_acc = accounts::table
                    .filter(accounts::id.eq(&old.account_id))
                    .filter(accounts::user_id.eq(&claims.sub))
                    .first::<Account>(conn)?;
                diesel::update(accounts::table.filter(accounts::id.eq(&old_acc.id)))
                    .set((
                        accounts::balance.eq(old_acc.balance - old_delta),
                        accounts::updated_at.eq(now),
                    ))
                    .execute(conn)?;
            }
            if new_delta != 0 {
                diesel::update(accounts::table.filter(accounts::id.eq(&new_acc.id)))
                    .set((
                        accounts::balance.eq(new_acc.balance + new_delta),
                        accounts::updated_at.eq(now),
                    ))
                    .execute(conn)?;
            }
        }

        let changeset = TransactionUpdateChangeset {
            amount: new_amount,
            description: new_desc,
            date: new_date,
            type_: new_type,
            payment_method: new_pm,
            card_name: new_card,
            account_id: new_acc.id,
            category_id: new_cat_id,
            is_fixed: new_fixed,
            is_informational: new_info,
            credit_paid: new_credit_paid,
            needs_confirmation: new_needs_conf,
            updated_at: now,
        };

        diesel::update(
            transactions::table
                .filter(transactions::id.eq(&id))
                .filter(transactions::user_id.eq(&claims.sub))
                .filter(transactions::deleted_at.is_null()),
        )
        .set(&changeset)
        .execute(conn)?;

        if let Some(ref tids) = payload.tag_ids {
            replace_transaction_tags(conn, &id, tids)?;
        }

        load_owned_transaction(conn, &id, &claims.sub)
    })?;

    let mut hydrated = hydrate_transactions(&mut conn, vec![updated_row])?;
    Ok(HttpResponse::Ok().json(hydrated.remove(0)))
}

#[delete("/transactions/{id}")]
pub async fn delete_transaction(
    tx_id: web::Path<String>,
    pool: web::Data<SqlitePool>,
    claims: web::ReqData<Claims>,
) -> ApiResult<HttpResponse> {
    let id = tx_id.into_inner();
    let mut conn = pool.get()?;
    let now = now_utc();

    conn.transaction::<_, ApiError, _>(|conn| {
        let old = load_owned_transaction(conn, &id, &claims.sub)?;
        let old_delta = transaction_balance_delta(
            &old.type_,
            &old.payment_method,
            old.is_informational,
            old.amount,
        );
        if old_delta != 0 {
            let acc = accounts::table
                .filter(accounts::id.eq(&old.account_id))
                .filter(accounts::user_id.eq(&claims.sub))
                .first::<Account>(conn)?;
            diesel::update(accounts::table.filter(accounts::id.eq(&acc.id)))
                .set((
                    accounts::balance.eq(acc.balance - old_delta),
                    accounts::updated_at.eq(now),
                ))
                .execute(conn)?;
        }

        diesel::update(
            transactions::table
                .filter(transactions::id.eq(&id))
                .filter(transactions::user_id.eq(&claims.sub)),
        )
        .set((
            transactions::deleted_at.eq(Some(now)),
            transactions::updated_at.eq(now),
        ))
        .execute(conn)?;

        Ok(())
    })?;

    Ok(HttpResponse::NoContent().finish())
}

#[post("/credit/pay-invoice")]
pub async fn pay_credit_invoice(
    payload: web::Json<PayCreditInvoiceRequest>,
    pool: web::Data<SqlitePool>,
    claims: web::ReqData<Claims>,
) -> ApiResult<HttpResponse> {
    if payload.amount <= 0 {
        return Err(ApiError::BadRequest(
            "Valor do pagamento da fatura deve ser maior que zero".into(),
        ));
    }
    let card_name = payload.card_name.trim();
    if card_name.is_empty() {
        return Err(ApiError::BadRequest("card_name é obrigatório".into()));
    }

    let mut conn = pool.get()?;
    let now = now_utc();
    let pay_date = payload.date.unwrap_or(now);

    let transfer = conn.transaction::<_, ApiError, _>(|conn| {
        let from_acc = load_owned_account(conn, &payload.from_account_id, &claims.sub)?;

        if let Some(ref ids) = payload.transaction_ids {
            if !ids.is_empty() {
                let mut unique_ids = Vec::new();
                for tid in ids {
                    if !unique_ids.contains(tid) {
                        unique_ids.push(tid.clone());
                    }
                }
                let found: Vec<Transaction> = transactions::table
                    .filter(transactions::id.eq_any(&unique_ids))
                    .filter(transactions::user_id.eq(&claims.sub))
                    .filter(transactions::deleted_at.is_null())
                    .filter(transactions::payment_method.eq("credit"))
                    .load(conn)?;
                if found.len() != unique_ids.len() {
                    return Err(ApiError::NotFound(
                        "Uma ou mais transações da fatura não pertencem ao usuário".into(),
                    ));
                }
                diesel::update(
                    transactions::table
                        .filter(transactions::id.eq_any(&unique_ids))
                        .filter(transactions::user_id.eq(&claims.sub)),
                )
                .set((
                    transactions::credit_paid.eq(true),
                    transactions::updated_at.eq(now),
                ))
                .execute(conn)?;
            }
        } else {
            diesel::update(
                transactions::table
                    .filter(transactions::user_id.eq(&claims.sub))
                    .filter(transactions::deleted_at.is_null())
                    .filter(transactions::payment_method.eq("credit"))
                    .filter(transactions::credit_paid.eq(false))
                    .filter(transactions::card_name.eq(card_name))
                    .filter(transactions::date.le(pay_date)),
            )
            .set((
                transactions::credit_paid.eq(true),
                transactions::updated_at.eq(now),
            ))
            .execute(conn)?;
        }

        diesel::update(accounts::table.filter(accounts::id.eq(&from_acc.id)))
            .set((
                accounts::balance.eq(from_acc.balance - payload.amount),
                accounts::updated_at.eq(now),
            ))
            .execute(conn)?;

        let desc = payload
            .description
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| format!("Pagamento fatura {card_name}"));

        let tr = AccountTransfer {
            id: Uuid::new_v4().to_string(),
            from_account_id: from_acc.id,
            to_account_id: None,
            amount: payload.amount,
            description: Some(desc),
            date: pay_date,
            transfer_kind: "invoice_payment".to_string(),
            user_id: claims.sub.clone(),
            deleted_at: None,
            created_at: now,
            updated_at: now,
        };
        diesel::insert_into(account_transfers::table)
            .values(&tr)
            .execute(conn)?;

        Ok(tr)
    })?;

    let mut hydrated = hydrate_transfers(&mut conn, vec![transfer])?;
    Ok(HttpResponse::Created().json(hydrated.remove(0)))
}
