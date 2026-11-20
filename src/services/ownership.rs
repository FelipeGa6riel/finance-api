use crate::errors::ApiResult;
use crate::models::{
    Account, AccountTransfer, Category, RecurringRule, Tag, TagResponse, Transaction,
    TransactionResponse, TransactionTag, TransferResponse, TransferTag, User,
};
use crate::schema::{
    account_transfers, accounts, categories, recurring_rules, tags, transaction_tags,
    transactions, transfer_tags, users,
};
use diesel::prelude::*;
use std::collections::HashMap;

pub fn load_active_user(conn: &mut SqliteConnection, user_id: &str) -> ApiResult<User> {
    let u = users::table
        .filter(users::id.eq(user_id))
        .filter(users::deleted_at.is_null())
        .first::<User>(conn)?;
    Ok(u)
}

pub fn load_owned_account(
    conn: &mut SqliteConnection,
    account_id: &str,
    user_id: &str,
) -> ApiResult<Account> {
    let acc = accounts::table
        .filter(accounts::id.eq(account_id))
        .filter(accounts::user_id.eq(user_id))
        .filter(accounts::deleted_at.is_null())
        .first::<Account>(conn)?;
    Ok(acc)
}

pub fn load_owned_category(
    conn: &mut SqliteConnection,
    category_id: &str,
    user_id: &str,
) -> ApiResult<Category> {
    let cat = categories::table
        .filter(categories::id.eq(category_id))
        .filter(categories::user_id.eq(user_id))
        .filter(categories::deleted_at.is_null())
        .first::<Category>(conn)?;
    Ok(cat)
}

pub fn load_owned_recurring(
    conn: &mut SqliteConnection,
    rule_id: &str,
    user_id: &str,
) -> ApiResult<RecurringRule> {
    let rule = recurring_rules::table
        .filter(recurring_rules::id.eq(rule_id))
        .filter(recurring_rules::user_id.eq(user_id))
        .filter(recurring_rules::deleted_at.is_null())
        .first::<RecurringRule>(conn)?;
    Ok(rule)
}

pub fn validate_owned_tags(
    conn: &mut SqliteConnection,
    tag_ids: &[String],
    user_id: &str,
) -> ApiResult<Vec<Tag>> {
    if tag_ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut unique_ids: Vec<String> = Vec::new();
    for tid in tag_ids {
        if !unique_ids.contains(tid) {
            unique_ids.push(tid.clone());
        }
    }
    let found: Vec<Tag> = tags::table
        .filter(tags::id.eq_any(&unique_ids))
        .filter(tags::user_id.eq(user_id))
        .filter(tags::deleted_at.is_null())
        .load(conn)?;

    if found.len() != unique_ids.len() {
        return Err(crate::errors::ApiError::NotFound(
            "Uma ou mais tags não pertencem ao usuário ou não existem".into(),
        ));
    }
    Ok(found)
}

pub fn replace_transaction_tags(
    conn: &mut SqliteConnection,
    tx_id: &str,
    tag_ids: &[String],
) -> ApiResult<()> {
    diesel::delete(transaction_tags::table.filter(transaction_tags::transaction_id.eq(tx_id)))
        .execute(conn)?;
    let mut seen = Vec::new();
    for tid in tag_ids {
        if seen.contains(tid) {
            continue;
        }
        seen.push(tid.clone());
        diesel::insert_into(transaction_tags::table)
            .values(TransactionTag {
                transaction_id: tx_id.to_string(),
                tag_id: tid.clone(),
            })
            .execute(conn)?;
    }
    Ok(())
}

pub fn replace_transfer_tags(
    conn: &mut SqliteConnection,
    tr_id: &str,
    tag_ids: &[String],
) -> ApiResult<()> {
    diesel::delete(transfer_tags::table.filter(transfer_tags::transfer_id.eq(tr_id)))
        .execute(conn)?;
    let mut seen = Vec::new();
    for tid in tag_ids {
        if seen.contains(tid) {
            continue;
        }
        seen.push(tid.clone());
        diesel::insert_into(transfer_tags::table)
            .values(TransferTag {
                transfer_id: tr_id.to_string(),
                tag_id: tid.clone(),
            })
            .execute(conn)?;
    }
    Ok(())
}

pub fn hydrate_transactions(
    conn: &mut SqliteConnection,
    txs: Vec<Transaction>,
) -> ApiResult<Vec<TransactionResponse>> {
    if txs.is_empty() {
        return Ok(Vec::new());
    }
    let ids: Vec<String> = txs.iter().map(|t| t.id.clone()).collect();
    let rows: Vec<(String, Tag)> = transaction_tags::table
        .inner_join(tags::table.on(tags::id.eq(transaction_tags::tag_id)))
        .filter(transaction_tags::transaction_id.eq_any(&ids))
        .filter(tags::deleted_at.is_null())
        .select((transaction_tags::transaction_id, Tag::as_select()))
        .load(conn)?;
    let mut map: HashMap<String, Vec<TagResponse>> = HashMap::new();
    for (tx_id, tag) in rows {
        map.entry(tx_id).or_default().push(tag.into());
    }
    Ok(txs
        .into_iter()
        .map(|tx| {
            let t = map.remove(&tx.id).unwrap_or_default();
            TransactionResponse::from_row(tx, t)
        })
        .collect())
}

pub fn hydrate_transfers(
    conn: &mut SqliteConnection,
    trs: Vec<AccountTransfer>,
) -> ApiResult<Vec<TransferResponse>> {
    if trs.is_empty() {
        return Ok(Vec::new());
    }
    let ids: Vec<String> = trs.iter().map(|t| t.id.clone()).collect();
    let rows: Vec<(String, Tag)> = transfer_tags::table
        .inner_join(tags::table.on(tags::id.eq(transfer_tags::tag_id)))
        .filter(transfer_tags::transfer_id.eq_any(&ids))
        .filter(tags::deleted_at.is_null())
        .select((transfer_tags::transfer_id, Tag::as_select()))
        .load(conn)?;
    let mut map: HashMap<String, Vec<TagResponse>> = HashMap::new();
    for (tr_id, tag) in rows {
        map.entry(tr_id).or_default().push(tag.into());
    }
    Ok(trs
        .into_iter()
        .map(|tr| {
            let t = map.remove(&tr.id).unwrap_or_default();
            TransferResponse::from_row(tr, t)
        })
        .collect())
}

pub fn load_owned_transaction(
    conn: &mut SqliteConnection,
    tx_id: &str,
    user_id: &str,
) -> ApiResult<Transaction> {
    let tx = transactions::table
        .filter(transactions::id.eq(tx_id))
        .filter(transactions::user_id.eq(user_id))
        .filter(transactions::deleted_at.is_null())
        .first::<Transaction>(conn)?;
    Ok(tx)
}

pub fn load_owned_transfer(
    conn: &mut SqliteConnection,
    tr_id: &str,
    user_id: &str,
) -> ApiResult<AccountTransfer> {
    let tr = account_transfers::table
        .filter(account_transfers::id.eq(tr_id))
        .filter(account_transfers::user_id.eq(user_id))
        .filter(account_transfers::deleted_at.is_null())
        .first::<AccountTransfer>(conn)?;
    Ok(tr)
}
