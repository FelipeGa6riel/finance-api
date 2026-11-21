use crate::db::now_utc;
use crate::errors::{ApiError, ApiResult};
use crate::models::{
    Account, AccountTransfer, Category, CycleConfig, RecurringRule, Tag, TagResponse, Transaction,
    Type, User, format_ymd, parse_flexible_datetime, parse_ymd, transaction_balance_delta,
};
use crate::schema::{
    account_transfers, accounts, categories, cycle_configs, recurring_rules, tags,
    transaction_tags, transactions, transfer_tags,
};
use diesel::prelude::*;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use time::{Date, Month, PrimitiveDateTime, Time};
use uuid::Uuid;

#[derive(Serialize, Clone, Debug)]
pub struct CycleInfo {
    pub cycle_key: String,
    pub start_date: String,
    pub end_date: String,
    pub label: String,
    pub has_custom_config: bool,
}

#[derive(Serialize, Clone, Debug)]
pub struct CategoryBreakdown {
    pub category_id: Option<String>,
    pub name: String,
    pub color: String,
    pub total: i64,
    pub percent: f64,
}

#[derive(Serialize, Clone, Debug)]
pub struct NamedTotal {
    pub name: String,
    pub total: i64,
    pub percent: f64,
}

#[derive(Serialize, Clone, Debug)]
pub struct DailyPoint {
    pub date: String,
    pub total: i64,
    pub cumulative: i64,
}

#[derive(Serialize, Clone, Debug)]
pub struct TopExpenseItem {
    pub id: String,
    pub date: String,
    pub description: String,
    pub category_name: String,
    pub amount: i64,
}

#[derive(Serialize, Clone, Debug)]
pub struct OpenCreditInvoice {
    pub card_name: String,
    pub total: i64,
    pub count: usize,
    pub transaction_ids: Vec<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct CycleSummary {
    pub cycle_key: String,
    pub start_date: String,
    pub end_date: String,
    pub label: String,
    pub notes: Option<String>,
    pub savings_mode: String,
    pub configured_savings_percent: Option<f32>,
    pub configured_savings_fixed_amount: Option<i64>,
    pub salary_base: i64,
    pub other_income: i64,
    pub total_income: i64,
    pub saved_amount: i64,
    pub saved_percent: f64,
    pub salary_after_savings: i64,
    pub available_to_spend: i64,
    pub total_expenses: i64,
    pub free_remainder: i64,
    pub pct_consumed_of_available: f64,
    pub investments_total: i64,
    pub investments_renda_fixa: i64,
    pub investments_renda_variavel: i64,
    pub investments_other: i64,
    pub total_saved_and_invested: i64,
    pub savings_rate_percent: f64,
    pub casa_total: i64,
    pub casa_percent: f64,
    pub fixed_total: i64,
    pub variable_total: i64,
    pub fixed_percent: f64,
    pub daily_average: i64,
    pub previous_cycle_key: String,
    pub previous_cycle_expenses: Option<i64>,
    pub mom_expense_diff_percent: Option<f64>,
    pub pending_confirmations_count: usize,
    pub by_category: Vec<CategoryBreakdown>,
    pub by_payment_method: Vec<NamedTotal>,
    pub by_card: Vec<NamedTotal>,
    pub daily_expenses: Vec<DailyPoint>,
    pub top_expenses: Vec<TopExpenseItem>,
    pub open_credit_invoices: Vec<OpenCreditInvoice>,
}

#[derive(Serialize, Clone, Debug)]
pub struct OverviewKpis {
    pub total_income: i64,
    pub total_consumption: i64,
    pub months_count: usize,
    pub months_with_spending: usize,
    pub monthly_average: i64,
    pub total_saved_and_invested: i64,
    pub savings_rate_percent: f64,
    pub accumulated_free_remainder: i64,
    pub total_casa: i64,
    pub casa_percent_of_consumption: f64,
}

#[derive(Serialize, Clone, Debug)]
pub struct MonthlyEvolutionPoint {
    pub cycle_key: String,
    pub label: String,
    pub income: i64,
    pub expenses: i64,
    pub saved: i64,
    pub free_remainder: i64,
}

#[derive(Serialize, Clone, Debug)]
pub struct CategoryTrendPoint {
    pub cycle_key: String,
    pub label: String,
    pub totals: BTreeMap<String, i64>,
}

#[derive(Serialize, Clone, Debug)]
pub struct InvestmentsSavingsPoint {
    pub cycle_key: String,
    pub label: String,
    pub saved: i64,
    pub renda_fixa: i64,
    pub renda_variavel: i64,
    pub other_investments: i64,
}

#[derive(Serialize, Clone, Debug)]
pub struct OverviewResponse {
    pub kpis: OverviewKpis,
    pub monthly_evolution: Vec<MonthlyEvolutionPoint>,
    pub top5_categories: Vec<String>,
    pub categories_trend: Vec<CategoryTrendPoint>,
    pub investments_savings: Vec<InvestmentsSavingsPoint>,
    pub payment_by_method: Vec<NamedTotal>,
    pub payment_by_card: Vec<NamedTotal>,
}

pub fn last_day_of_month(year: i32, month: Month) -> u8 {
    let (ny, nm) = if month == Month::December {
        (year + 1, Month::January)
    } else {
        (year, month.next())
    };
    Date::from_calendar_date(ny, nm, 1)
        .expect("valid next month")
        .previous_day()
        .expect("valid prev day")
        .day()
}

pub fn parse_cycle_key(cycle_key: &str) -> Result<(i32, Month), String> {
    let parts: Vec<&str> = cycle_key.trim().split('-').collect();
    if parts.len() != 2 || parts[0].len() != 4 || parts[1].len() != 2 {
        return Err(format!("cycle_key inválido (esperado YYYY-MM): '{cycle_key}'"));
    }
    let y: i32 = parts[0]
        .parse()
        .map_err(|_| format!("Ano inválido em cycle_key: '{cycle_key}'"))?;
    let m: u8 = parts[1]
        .parse()
        .map_err(|_| format!("Mês inválido em cycle_key: '{cycle_key}'"))?;
    let month = Month::try_from(m)
        .map_err(|_| format!("Mês inválido em cycle_key: '{cycle_key}'"))?;
    Ok((y, month))
}

pub fn cycle_bounds_for_key(cycle_key: &str, cycle_start_day: i32) -> Result<(Date, Date), String> {
    let (year, month) = parse_cycle_key(cycle_key)?;
    let start_day = cycle_start_day.clamp(1, 28) as u8;
    if start_day == 1 {
        let start = Date::from_calendar_date(year, month, 1).map_err(|e| e.to_string())?;
        let end = Date::from_calendar_date(year, month, last_day_of_month(year, month))
            .map_err(|e| e.to_string())?;
        return Ok((start, end));
    }

    let (py, pm) = if month == Month::January {
        (year - 1, Month::December)
    } else {
        (year, month.previous())
    };
    let s_day = start_day.min(last_day_of_month(py, pm));
    let e_day = (start_day - 1).min(last_day_of_month(year, month));
    let start = Date::from_calendar_date(py, pm, s_day).map_err(|e| e.to_string())?;
    let end = Date::from_calendar_date(year, month, e_day).map_err(|e| e.to_string())?;
    Ok((start, end))
}

pub fn cycle_key_for_date(date: Date, cycle_start_day: i32) -> String {
    let start_day = cycle_start_day.clamp(1, 28) as u8;
    if start_day == 1 {
        return format!("{:04}-{:02}", date.year(), date.month() as u8);
    }
    let clamped = start_day.min(last_day_of_month(date.year(), date.month()));
    if date.day() >= clamped {
        let (ny, nm) = if date.month() == Month::December {
            (date.year() + 1, Month::January)
        } else {
            (date.year(), date.month().next())
        };
        format!("{:04}-{:02}", ny, nm as u8)
    } else {
        format!("{:04}-{:02}", date.year(), date.month() as u8)
    }
}

pub fn previous_cycle_key(cycle_key: &str) -> Result<String, String> {
    let (year, month) = parse_cycle_key(cycle_key)?;
    let (py, pm) = if month == Month::January {
        (year - 1, Month::December)
    } else {
        (year, month.previous())
    };
    Ok(format!("{:04}-{:02}", py, pm as u8))
}

pub fn clean_cycle_label(raw: &str) -> String {
    let mut t = raw.trim();
    for prefix in [
        "Rastreador Financeiro - ",
        "rastreador financeiro - ",
        "Rastreador Financeiro -",
        "Financeiro - ",
        "financeiro - ",
        "Financeiro -",
    ] {
        if let Some(rest) = t.strip_prefix(prefix) {
            t = rest.trim();
            break;
        }
    }
    t.to_string()
}

pub fn default_cycle_label(start: Date, end: Date) -> String {
    if start.year() == end.year() {
        format!(
            "{:02}/{:02} – {:02}/{:02}/{}",
            start.day(),
            start.month() as u8,
            end.day(),
            end.month() as u8,
            end.year()
        )
    } else {
        format!(
            "{:02}/{:02}/{} – {:02}/{:02}/{}",
            start.day(),
            start.month() as u8,
            start.year(),
            end.day(),
            end.month() as u8,
            end.year()
        )
    }
}

fn parse_bound_start(raw: &str) -> Result<PrimitiveDateTime, ApiError> {
    let s = raw.trim();
    if s.len() == 10 {
        let d = parse_ymd(s).map_err(ApiError::BadRequest)?;
        Ok(PrimitiveDateTime::new(d, Time::MIDNIGHT))
    } else {
        parse_flexible_datetime(s).map_err(ApiError::BadRequest)
    }
}

fn parse_bound_end(raw: &str) -> Result<PrimitiveDateTime, ApiError> {
    let s = raw.trim();
    if s.len() == 10 {
        let d = parse_ymd(s).map_err(ApiError::BadRequest)?;
        let t = Time::from_hms(23, 59, 59).expect("valid end time");
        Ok(PrimitiveDateTime::new(d, t))
    } else {
        parse_flexible_datetime(s).map_err(ApiError::BadRequest)
    }
}

pub fn resolve_cycle_window(
    conn: &mut SqliteConnection,
    user: &User,
    cycle_key: &str,
) -> ApiResult<(PrimitiveDateTime, PrimitiveDateTime, String, Option<CycleConfig>)> {
    let cfg = cycle_configs::table
        .filter(cycle_configs::user_id.eq(&user.id))
        .filter(cycle_configs::cycle_key.eq(cycle_key))
        .first::<CycleConfig>(conn)
        .optional()?;

    if let Some(ref c) = cfg {
        let start_dt = parse_bound_start(&c.start_date)?;
        let end_dt = parse_bound_end(&c.end_date)?;
        let lbl = c
            .label
            .as_deref()
            .map(clean_cycle_label)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| default_cycle_label(start_dt.date(), end_dt.date()));
        return Ok((start_dt, end_dt, lbl, cfg));
    }

    let (start_d, end_d) =
        cycle_bounds_for_key(cycle_key, user.cycle_start_day).map_err(ApiError::BadRequest)?;
    let start_dt = PrimitiveDateTime::new(start_d, Time::MIDNIGHT);
    let end_dt = PrimitiveDateTime::new(end_d, Time::from_hms(23, 59, 59).expect("valid time"));
    let lbl = default_cycle_label(start_d, end_d);
    Ok((start_dt, end_dt, lbl, None))
}

pub fn list_user_cycles(conn: &mut SqliteConnection, user: &User) -> ApiResult<Vec<CycleInfo>> {
    let configs: Vec<CycleConfig> = cycle_configs::table
        .filter(cycle_configs::user_id.eq(&user.id))
        .load(conn)?;

    let mut config_map: HashMap<String, CycleConfig> = HashMap::new();
    let mut keys: BTreeSet<String> = BTreeSet::new();

    for c in configs {
        keys.insert(c.cycle_key.clone());
        config_map.insert(c.cycle_key.clone(), c);
    }

    let current_key = cycle_key_for_date(now_utc().date(), user.cycle_start_day);
    keys.insert(current_key);

    let tx_dates: Vec<PrimitiveDateTime> = transactions::table
        .filter(transactions::user_id.eq(&user.id))
        .filter(transactions::deleted_at.is_null())
        .select(transactions::date)
        .load(conn)?;

    let tr_dates: Vec<PrimitiveDateTime> = account_transfers::table
        .filter(account_transfers::user_id.eq(&user.id))
        .filter(account_transfers::deleted_at.is_null())
        .select(account_transfers::date)
        .load(conn)?;

    // Only infer cycle_key for dates that don't already fall into an explicit cycle_config
    for dt in tx_dates.into_iter().chain(tr_dates.into_iter()) {
        let matched_config = config_map.values().any(|c| {
            if let (Ok(s), Ok(e)) = (parse_bound_start(&c.start_date), parse_bound_end(&c.end_date)) {
                dt >= s && dt <= e
            } else {
                false
            }
        });
        if !matched_config {
            keys.insert(cycle_key_for_date(dt.date(), user.cycle_start_day));
        }
    }

    let mut out = Vec::with_capacity(keys.len());
    for key in keys {
        if let Some(c) = config_map.get(&key) {
            let s_d = parse_bound_start(&c.start_date)
                .map(|dt| format_ymd(dt.date()))
                .unwrap_or_else(|_| c.start_date.clone());
            let e_d = parse_bound_end(&c.end_date)
                .map(|dt| format_ymd(dt.date()))
                .unwrap_or_else(|_| c.end_date.clone());
            let label = c
                .label
                .as_deref()
                .map(clean_cycle_label)
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| {
                    if let (Ok(sd), Ok(ed)) = (parse_ymd(&s_d), parse_ymd(&e_d)) {
                        default_cycle_label(sd, ed)
                    } else {
                        key.clone()
                    }
                });
            out.push(CycleInfo {
                cycle_key: key,
                start_date: s_d,
                end_date: e_d,
                label,
                has_custom_config: true,
            });
        } else if let Ok((sd, ed)) = cycle_bounds_for_key(&key, user.cycle_start_day) {
            out.push(CycleInfo {
                cycle_key: key,
                start_date: format_ymd(sd),
                end_date: format_ymd(ed),
                label: default_cycle_label(sd, ed),
                has_custom_config: false,
            });
        }
    }

    Ok(out)
}

fn recurrence_date_in_window(start: Date, end: Date, day_of_month: i32) -> Date {
    let target = day_of_month.clamp(1, 31) as u8;
    let mut cursor = start;
    while cursor <= end {
        let clamped = target.min(last_day_of_month(cursor.year(), cursor.month()));
        if cursor.day() == clamped {
            return cursor;
        }
        match cursor.next_day() {
            Some(next) => cursor = next,
            None => break,
        }
    }
    end
}

pub fn ensure_cycle_recurrences(
    conn: &mut SqliteConnection,
    user_id: &str,
    start_dt: PrimitiveDateTime,
    end_dt: PrimitiveDateTime,
) -> ApiResult<usize> {
    let rules: Vec<RecurringRule> = recurring_rules::table
        .filter(recurring_rules::user_id.eq(user_id))
        .filter(recurring_rules::active.eq(true))
        .filter(recurring_rules::deleted_at.is_null())
        .load(conn)?;

    if rules.is_empty() {
        return Ok(0);
    }

    let now = now_utc();
    let mut created_count = 0usize;

    conn.transaction::<_, ApiError, _>(|conn| {
        for rule in &rules {
            let existing: Option<String> = transactions::table
                .filter(transactions::user_id.eq(user_id))
                .filter(transactions::recurring_rule_id.eq(&rule.id))
                .filter(transactions::date.ge(start_dt))
                .filter(transactions::date.le(end_dt))
                .select(transactions::id)
                .first(conn)
                .optional()?;

            if existing.is_some() {
                continue;
            }

            // Ensure target account is still active and owned by user
            let account: Option<Account> = accounts::table
                .filter(accounts::id.eq(&rule.account_id))
                .filter(accounts::user_id.eq(user_id))
                .filter(accounts::deleted_at.is_null())
                .first(conn)
                .optional()?;

            let Some(account) = account else {
                continue;
            };

            let due_date = recurrence_date_in_window(start_dt.date(), end_dt.date(), rule.day_of_month);
            let due_dt = PrimitiveDateTime::new(
                due_date,
                Time::from_hms(9, 0, 0).expect("valid time"),
            );

            let delta = transaction_balance_delta(
                &rule.type_,
                &rule.payment_method,
                false,
                rule.default_amount,
            );
            if delta != 0 {
                diesel::update(accounts::table.filter(accounts::id.eq(&account.id)))
                    .set((
                        accounts::balance.eq(account.balance + delta),
                        accounts::updated_at.eq(now),
                    ))
                    .execute(conn)?;
            }

            let new_tx = Transaction {
                id: Uuid::new_v4().to_string(),
                amount: rule.default_amount,
                description: Some(rule.description.clone()),
                date: due_dt,
                type_: rule.type_,
                payment_method: rule.payment_method.clone(),
                card_name: None,
                account_id: rule.account_id.clone(),
                transfer_id: None,
                category_id: rule.category_id.clone(),
                is_fixed: Some(rule.is_fixed),
                is_informational: false,
                credit_paid: false,
                installment_group_id: None,
                installment_number: None,
                installment_total: None,
                recurring_rule_id: Some(rule.id.clone()),
                needs_confirmation: rule.is_variable_amount,
                user_id: user_id.to_string(),
                deleted_at: None,
                created_at: now,
                updated_at: now,
            };

            diesel::insert_into(transactions::table)
                .values(&new_tx)
                .execute(conn)?;
            created_count += 1;
        }
        Ok(())
    })?;

    Ok(created_count)
}

fn load_tx_tags_map(
    conn: &mut SqliteConnection,
    tx_ids: &[String],
) -> ApiResult<HashMap<String, Vec<TagResponse>>> {
    let mut map: HashMap<String, Vec<TagResponse>> = HashMap::new();
    if tx_ids.is_empty() {
        return Ok(map);
    }
    let rows: Vec<(String, Tag)> = transaction_tags::table
        .inner_join(tags::table.on(tags::id.eq(transaction_tags::tag_id)))
        .filter(transaction_tags::transaction_id.eq_any(tx_ids))
        .filter(tags::deleted_at.is_null())
        .select((transaction_tags::transaction_id, Tag::as_select()))
        .load(conn)?;
    for (tx_id, tag) in rows {
        map.entry(tx_id).or_default().push(tag.into());
    }
    Ok(map)
}

fn load_transfer_tags_map(
    conn: &mut SqliteConnection,
    tr_ids: &[String],
) -> ApiResult<HashMap<String, Vec<TagResponse>>> {
    let mut map: HashMap<String, Vec<TagResponse>> = HashMap::new();
    if tr_ids.is_empty() {
        return Ok(map);
    }
    let rows: Vec<(String, Tag)> = transfer_tags::table
        .inner_join(tags::table.on(tags::id.eq(transfer_tags::tag_id)))
        .filter(transfer_tags::transfer_id.eq_any(tr_ids))
        .filter(tags::deleted_at.is_null())
        .select((transfer_tags::transfer_id, Tag::as_select()))
        .load(conn)?;
    for (tr_id, tag) in rows {
        map.entry(tr_id).or_default().push(tag.into());
    }
    Ok(map)
}

const DEFAULT_CATEGORY_PALETTE: &[&str] = &[
    "#006d43", "#36a2eb", "#ff9f40", "#9966ff", "#ec407a", "#4bc0c0", "#8bc34a", "#607d8b",
];

pub fn compute_cycle_summary(
    conn: &mut SqliteConnection,
    user: &User,
    cycle_key: &str,
    include_mom: bool,
    auto_generate_recurrences: bool,
) -> ApiResult<CycleSummary> {
    let (start_dt, end_dt, label, cfg) = resolve_cycle_window(conn, user, cycle_key)?;

    if auto_generate_recurrences {
        let current_key = cycle_key_for_date(now_utc().date(), user.cycle_start_day);
        if cycle_key == current_key {
            let _ = ensure_cycle_recurrences(conn, &user.id, start_dt, end_dt)?;
        }
    }

    let txs: Vec<Transaction> = transactions::table
        .filter(transactions::user_id.eq(&user.id))
        .filter(transactions::deleted_at.is_null())
        .filter(transactions::date.ge(start_dt))
        .filter(transactions::date.le(end_dt))
        .order(transactions::date.asc())
        .load(conn)?;

    let tx_ids: Vec<String> = txs.iter().map(|t| t.id.clone()).collect();
    let tx_tags_map = load_tx_tags_map(conn, &tx_ids)?;

    let transfers: Vec<AccountTransfer> = account_transfers::table
        .filter(account_transfers::user_id.eq(&user.id))
        .filter(account_transfers::deleted_at.is_null())
        .filter(account_transfers::date.ge(start_dt))
        .filter(account_transfers::date.le(end_dt))
        .load(conn)?;

    let tr_ids: Vec<String> = transfers.iter().map(|t| t.id.clone()).collect();
    let tr_tags_map = load_transfer_tags_map(conn, &tr_ids)?;

    let user_cats: Vec<Category> = categories::table
        .filter(categories::user_id.eq(&user.id))
        .load(conn)?;
    let cat_map: HashMap<String, Category> =
        user_cats.into_iter().map(|c| (c.id.clone(), c)).collect();

    let mut salary_base: i64 = 0;
    let mut other_income: i64 = 0;
    let mut total_expenses: i64 = 0;
    let mut casa_total: i64 = 0;
    let mut fixed_total: i64 = 0;
    let mut variable_total: i64 = 0;
    let mut pending_confirmations_count: usize = 0;

    let mut cat_totals: HashMap<Option<String>, i64> = HashMap::new();
    let mut method_totals: HashMap<String, i64> = HashMap::new();
    let mut card_totals: HashMap<String, i64> = HashMap::new();
    let mut daily_totals: BTreeMap<String, i64> = BTreeMap::new();
    let mut expense_items: Vec<TopExpenseItem> = Vec::new();
    let mut open_invoices_map: HashMap<String, (i64, Vec<String>)> = HashMap::new();

    for tx in &txs {
        if tx.needs_confirmation {
            pending_confirmations_count += 1;
        }
        let cat = tx.category_id.as_ref().and_then(|cid| cat_map.get(cid));
        let tags = tx_tags_map.get(&tx.id);

        match tx.type_ {
            Type::Income => {
                let is_salary = cat.map(|c| c.is_salary).unwrap_or(false)
                    || (tx.category_id.is_none() && tx.payment_method == "salary_account");
                if is_salary {
                    salary_base += tx.amount;
                } else {
                    other_income += tx.amount;
                }
            }
            Type::Expense => {
                if tx.payment_method == "credit" && !tx.is_informational && !tx.credit_paid && tx.amount > 0 {
                    let cname = tx
                        .card_name
                        .clone()
                        .filter(|s| !s.trim().is_empty())
                        .unwrap_or_else(|| "Cartão".to_string());
                    let entry = open_invoices_map.entry(cname).or_insert((0, Vec::new()));
                    entry.0 += tx.amount;
                    entry.1.push(tx.id.clone());
                }

                if tx.is_informational || tx.amount <= 0 {
                    continue;
                }

                total_expenses += tx.amount;

                let has_casa_tag = tags
                    .map(|list| list.iter().any(|t| t.name.eq_ignore_ascii_case("casa")))
                    .unwrap_or(false);
                let is_casa_cat = cat
                    .map(|c| c.name.eq_ignore_ascii_case("casa"))
                    .unwrap_or(false);
                if has_casa_tag || is_casa_cat {
                    casa_total += tx.amount;
                }

                let is_fixed_eff = tx
                    .is_fixed
                    .unwrap_or_else(|| cat.map(|c| c.is_fixed).unwrap_or(false));
                if is_fixed_eff {
                    fixed_total += tx.amount;
                } else {
                    variable_total += tx.amount;
                }

                *cat_totals.entry(tx.category_id.clone()).or_insert(0) += tx.amount;
                *method_totals.entry(tx.payment_method.clone()).or_insert(0) += tx.amount;

                if let Some(ref cname) = tx.card_name {
                    let trimmed = cname.trim();
                    if !trimmed.is_empty() {
                        *card_totals.entry(trimmed.to_string()).or_insert(0) += tx.amount;
                    }
                }

                let day_str = format_ymd(tx.date.date());
                *daily_totals.entry(day_str.clone()).or_insert(0) += tx.amount;

                expense_items.push(TopExpenseItem {
                    id: tx.id.clone(),
                    date: day_str,
                    description: tx
                        .description
                        .clone()
                        .unwrap_or_else(|| "Sem descrição".to_string()),
                    category_name: cat
                        .map(|c| c.name.clone())
                        .unwrap_or_else(|| "Sem categoria".to_string()),
                    amount: tx.amount,
                });
            }
            Type::Transfer => {}
        }
    }

    let mut investments_renda_fixa: i64 = 0;
    let mut investments_renda_variavel: i64 = 0;
    let mut investments_other: i64 = 0;
    let mut savings_transfers_total: i64 = 0;

    for tr in &transfers {
        if tr.transfer_kind == "investment" {
            let tags = tr_tags_map.get(&tr.id);
            let is_rf = tags
                .map(|l| l.iter().any(|t| t.name.eq_ignore_ascii_case("renda_fixa")))
                .unwrap_or(false);
            let is_rv = tags
                .map(|l| l.iter().any(|t| t.name.eq_ignore_ascii_case("renda_variavel")))
                .unwrap_or(false);
            if is_rf {
                investments_renda_fixa += tr.amount;
            } else if is_rv {
                investments_renda_variavel += tr.amount;
            } else {
                investments_other += tr.amount;
            }
        } else if tr.transfer_kind == "savings" {
            savings_transfers_total += tr.amount;
        }
    }

    let investments_total = investments_renda_fixa + investments_renda_variavel + investments_other;
    let total_income = salary_base + other_income;

    let (savings_mode, configured_savings_percent, configured_savings_fixed_amount, saved_amount) =
        if let Some(ref c) = cfg {
            if c.savings_mode == "fixed" {
                let amt = c.savings_fixed_amount.unwrap_or(0);
                ("fixed".to_string(), c.savings_percent, c.savings_fixed_amount, amt)
            } else {
                let pct = c.savings_percent.unwrap_or(user.default_savings_percent);
                let amt = ((total_income as f64) * (pct as f64) / 100.0).round() as i64;
                ("percent".to_string(), Some(pct), c.savings_fixed_amount, amt)
            }
        } else if savings_transfers_total > 0 {
            ("transfer".to_string(), None, None, savings_transfers_total)
        } else {
            let pct = user.default_savings_percent;
            let amt = ((total_income as f64) * (pct as f64) / 100.0).round() as i64;
            ("percent".to_string(), Some(pct), None, amt)
        };

    let saved_percent = if total_income > 0 {
        (saved_amount as f64 / total_income as f64) * 100.0
    } else {
        0.0
    };
    let salary_after_savings = salary_base - saved_amount;
    let available_to_spend = total_income - saved_amount;
    let free_remainder = available_to_spend - total_expenses;
    let pct_consumed_of_available = if available_to_spend > 0 {
        ((total_expenses as f64 / available_to_spend as f64) * 100.0).clamp(0.0, 100.0)
    } else if total_expenses > 0 {
        100.0
    } else {
        0.0
    };

    let total_saved_and_invested = saved_amount + investments_total;
    let savings_rate_percent = if total_income > 0 {
        (total_saved_and_invested as f64 / total_income as f64) * 100.0
    } else {
        0.0
    };
    let casa_percent = if total_expenses > 0 {
        (casa_total as f64 / total_expenses as f64) * 100.0
    } else {
        0.0
    };
    let fixed_percent = if total_expenses > 0 {
        (fixed_total as f64 / total_expenses as f64) * 100.0
    } else {
        0.0
    };

    let mut by_category: Vec<CategoryBreakdown> = cat_totals
        .into_iter()
        .enumerate()
        .map(|(idx, (cid, total))| {
            let cat = cid.as_ref().and_then(|id| cat_map.get(id));
            let name = cat
                .map(|c| c.name.clone())
                .unwrap_or_else(|| "Sem categoria".to_string());
            let color = cat
                .and_then(|c| c.color.clone())
                .unwrap_or_else(|| DEFAULT_CATEGORY_PALETTE[idx % DEFAULT_CATEGORY_PALETTE.len()].to_string());
            let percent = if total_expenses > 0 {
                (total as f64 / total_expenses as f64) * 100.0
            } else {
                0.0
            };
            CategoryBreakdown {
                category_id: cid,
                name,
                color,
                total,
                percent,
            }
        })
        .collect();
    by_category.sort_by(|a, b| b.total.cmp(&a.total));

    let by_payment_method = to_named_totals(method_totals, total_expenses);
    let by_card = to_named_totals(card_totals, total_expenses);

    let mut cum: i64 = 0;
    let daily_expenses: Vec<DailyPoint> = daily_totals
        .into_iter()
        .map(|(date, total)| {
            cum += total;
            DailyPoint {
                date,
                total,
                cumulative: cum,
            }
        })
        .collect();

    let daily_average = if daily_expenses.is_empty() {
        0
    } else {
        total_expenses / (daily_expenses.len() as i64)
    };

    expense_items.sort_by(|a, b| b.amount.cmp(&a.amount));
    expense_items.truncate(5);

    let mut open_credit_invoices: Vec<OpenCreditInvoice> = open_invoices_map
        .into_iter()
        .map(|(card_name, (total, transaction_ids))| OpenCreditInvoice {
            card_name,
            total,
            count: transaction_ids.len(),
            transaction_ids,
        })
        .collect();
    open_credit_invoices.sort_by(|a, b| b.total.cmp(&a.total));

    let prev_key = previous_cycle_key(cycle_key).unwrap_or_else(|_| cycle_key.to_string());
    let (previous_cycle_expenses, mom_expense_diff_percent) = if include_mom && prev_key != cycle_key {
        match compute_cycle_summary(conn, user, &prev_key, false, false) {
            Ok(prev) if prev.total_expenses > 0 => {
                let diff = ((total_expenses as f64 - prev.total_expenses as f64)
                    / prev.total_expenses as f64)
                    * 100.0;
                (Some(prev.total_expenses), Some(diff))
            }
            _ => (None, None),
        }
    } else {
        (None, None)
    };

    Ok(CycleSummary {
        cycle_key: cycle_key.to_string(),
        start_date: format_ymd(start_dt.date()),
        end_date: format_ymd(end_dt.date()),
        label,
        notes: cfg.and_then(|c| c.notes),
        savings_mode,
        configured_savings_percent,
        configured_savings_fixed_amount,
        salary_base,
        other_income,
        total_income,
        saved_amount,
        saved_percent,
        salary_after_savings,
        available_to_spend,
        total_expenses,
        free_remainder,
        pct_consumed_of_available,
        investments_total,
        investments_renda_fixa,
        investments_renda_variavel,
        investments_other,
        total_saved_and_invested,
        savings_rate_percent,
        casa_total,
        casa_percent,
        fixed_total,
        variable_total,
        fixed_percent,
        daily_average,
        previous_cycle_key: prev_key,
        previous_cycle_expenses,
        mom_expense_diff_percent,
        pending_confirmations_count,
        by_category,
        by_payment_method,
        by_card,
        daily_expenses,
        top_expenses: expense_items,
        open_credit_invoices,
    })
}

fn to_named_totals(map: HashMap<String, i64>, denom: i64) -> Vec<NamedTotal> {
    let mut out: Vec<NamedTotal> = map
        .into_iter()
        .map(|(name, total)| NamedTotal {
            name,
            total,
            percent: if denom > 0 {
                (total as f64 / denom as f64) * 100.0
            } else {
                0.0
            },
        })
        .collect();
    out.sort_by(|a, b| b.total.cmp(&a.total));
    out
}

pub fn compute_overview(conn: &mut SqliteConnection, user: &User) -> ApiResult<OverviewResponse> {
    let cycles = list_user_cycles(conn, user)?;
    let mut summaries = Vec::with_capacity(cycles.len());
    for c in &cycles {
        summaries.push(compute_cycle_summary(conn, user, &c.cycle_key, false, false)?);
    }

    let mut total_income: i64 = 0;
    let mut total_consumption: i64 = 0;
    let mut total_saved_and_invested: i64 = 0;
    let mut total_casa: i64 = 0;
    let mut months_with_spending: usize = 0;

    let mut global_cat_totals: HashMap<String, i64> = HashMap::new();
    let mut global_methods: HashMap<String, i64> = HashMap::new();
    let mut global_cards: HashMap<String, i64> = HashMap::new();

    let mut monthly_evolution = Vec::with_capacity(summaries.len());
    let mut investments_savings = Vec::with_capacity(summaries.len());

    for s in &summaries {
        total_income += s.total_income;
        total_consumption += s.total_expenses;
        total_saved_and_invested += s.total_saved_and_invested;
        total_casa += s.casa_total;
        if s.total_expenses > 0 || s.investments_total > 0 {
            months_with_spending += 1;
        }

        for cat in &s.by_category {
            *global_cat_totals.entry(cat.name.clone()).or_insert(0) += cat.total;
        }
        for m in &s.by_payment_method {
            *global_methods.entry(m.name.clone()).or_insert(0) += m.total;
        }
        for c in &s.by_card {
            *global_cards.entry(c.name.clone()).or_insert(0) += c.total;
        }

        monthly_evolution.push(MonthlyEvolutionPoint {
            cycle_key: s.cycle_key.clone(),
            label: s.label.clone(),
            income: s.total_income,
            expenses: s.total_expenses,
            saved: s.saved_amount,
            free_remainder: s.free_remainder,
        });

        investments_savings.push(InvestmentsSavingsPoint {
            cycle_key: s.cycle_key.clone(),
            label: s.label.clone(),
            saved: s.saved_amount,
            renda_fixa: s.investments_renda_fixa,
            renda_variavel: s.investments_renda_variavel,
            other_investments: s.investments_other,
        });
    }

    let mut top_cats: Vec<(String, i64)> = global_cat_totals.into_iter().collect();
    top_cats.sort_by(|a, b| b.1.cmp(&a.1));
    let top5_categories: Vec<String> = top_cats.into_iter().take(5).map(|(n, _)| n).collect();

    let categories_trend: Vec<CategoryTrendPoint> = summaries
        .iter()
        .map(|s| {
            let mut totals = BTreeMap::new();
            for name in &top5_categories {
                let val = s
                    .by_category
                    .iter()
                    .find(|c| &c.name == name)
                    .map(|c| c.total)
                    .unwrap_or(0);
                totals.insert(name.clone(), val);
            }
            CategoryTrendPoint {
                cycle_key: s.cycle_key.clone(),
                label: s.label.clone(),
                totals,
            }
        })
        .collect();

    let monthly_average = if months_with_spending > 0 {
        total_consumption / (months_with_spending as i64)
    } else {
        0
    };
    let savings_rate_percent = if total_income > 0 {
        (total_saved_and_invested as f64 / total_income as f64) * 100.0
    } else {
        0.0
    };
    let accumulated_free_remainder = total_income - total_consumption - total_saved_and_invested;
    let casa_percent_of_consumption = if total_consumption > 0 {
        (total_casa as f64 / total_consumption as f64) * 100.0
    } else {
        0.0
    };

    Ok(OverviewResponse {
        kpis: OverviewKpis {
            total_income,
            total_consumption,
            months_count: summaries.len(),
            months_with_spending,
            monthly_average,
            total_saved_and_invested,
            savings_rate_percent,
            accumulated_free_remainder,
            total_casa,
            casa_percent_of_consumption,
        },
        monthly_evolution,
        top5_categories,
        categories_trend,
        investments_savings,
        payment_by_method: to_named_totals(global_methods, total_consumption),
        payment_by_card: to_named_totals(global_cards, total_consumption),
    })
}
