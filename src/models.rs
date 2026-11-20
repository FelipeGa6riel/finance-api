use diesel::deserialize::FromSql;
use diesel::prelude::*;
use diesel::serialize::{Output, ToSql};
use diesel::sql_types::Text;
use diesel::sqlite::Sqlite;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use time::{Date, Month, PrimitiveDateTime, Time};

pub mod flexible_datetime {
    use super::*;
    use time::format_description::FormatItem;
    use time::macros::format_description;

    const FMT: &[FormatItem<'static>] =
        format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");

    pub fn serialize<S>(dt: &PrimitiveDateTime, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = dt.format(FMT).map_err(serde::ser::Error::custom)?;
        serializer.serialize_str(&s)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<PrimitiveDateTime, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        parse_flexible_datetime(&s).map_err(serde::de::Error::custom)
    }

    pub mod option {
        use super::*;

        pub fn serialize<S>(
            dt: &Option<PrimitiveDateTime>,
            serializer: S,
        ) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            match dt {
                Some(d) => super::serialize(d, serializer),
                None => serializer.serialize_none(),
            }
        }

        pub fn deserialize<'de, D>(
            deserializer: D,
        ) -> Result<Option<PrimitiveDateTime>, D::Error>
        where
            D: Deserializer<'de>,
        {
            let opt = Option::<String>::deserialize(deserializer)?;
            match opt {
                Some(s) if !s.trim().is_empty() => {
                    parse_flexible_datetime(&s).map(Some).map_err(serde::de::Error::custom)
                }
                _ => Ok(None),
            }
        }
    }
}

pub fn parse_flexible_datetime(raw: &str) -> Result<PrimitiveDateTime, String> {
    let s = raw.trim();
    // Handle YYYY-MM-DD
    if s.len() == 10 && s.as_bytes()[4] == b'-' && s.as_bytes()[7] == b'-' {
        let d = parse_ymd(s)?;
        return Ok(PrimitiveDateTime::new(d, Time::MIDNIGHT));
    }
    // Strip trailing Z or fractional seconds or timezone offset if ISO-8601
    let normalized = s.replace('T', " ");
    let base = normalized.split('.').next().unwrap_or(&normalized);
    let base = base.trim_end_matches('Z');
    let base = if base.len() >= 19 { &base[..19] } else { base };
    let parts: Vec<&str> = base.split(' ').collect();
    if parts.len() != 2 {
        return Err(format!("Formato de data inválido: '{raw}'"));
    }
    let d = parse_ymd(parts[0])?;
    let t_parts: Vec<&str> = parts[1].split(':').collect();
    if t_parts.len() != 3 {
        return Err(format!("Formato de hora inválido: '{raw}'"));
    }
    let hh: u8 = t_parts[0].parse().map_err(|_| format!("Hora inválida: '{raw}'"))?;
    let mm: u8 = t_parts[1].parse().map_err(|_| format!("Minuto inválido: '{raw}'"))?;
    let ss: u8 = t_parts[2].parse().map_err(|_| format!("Segundo inválido: '{raw}'"))?;
    let t = Time::from_hms(hh, mm, ss).map_err(|e| format!("Hora inválida '{raw}': {e}"))?;
    Ok(PrimitiveDateTime::new(d, t))
}

pub fn parse_ymd(s: &str) -> Result<Date, String> {
    let parts: Vec<&str> = s.trim().split('-').collect();
    if parts.len() != 3 {
        return Err(format!("Data YYYY-MM-DD inválida: '{s}'"));
    }
    let y: i32 = parts[0].parse().map_err(|_| format!("Ano inválido: '{s}'"))?;
    let m: u8 = parts[1].parse().map_err(|_| format!("Mês inválido: '{s}'"))?;
    let d: u8 = parts[2].parse().map_err(|_| format!("Dia inválido: '{s}'"))?;
    let month = Month::try_from(m).map_err(|_| format!("Mês inválido: '{s}'"))?;
    Date::from_calendar_date(y, month, d).map_err(|e| format!("Data inválida '{s}': {e}"))
}

pub fn format_ymd(d: Date) -> String {
    format!("{:04}-{:02}-{:02}", d.year(), d.month() as u8, d.day())
}

pub fn deserialize_some<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Deserialize::deserialize(deserializer).map(Some)
}

#[derive(
    Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, diesel::AsExpression, diesel::FromSqlRow,
)]
#[diesel(sql_type = diesel::sql_types::Text)]
pub enum Type {
    #[serde(alias = "income")]
    Income,
    #[serde(alias = "expense")]
    Expense,
    #[serde(alias = "transfer")]
    Transfer,
}

impl ToSql<Text, Sqlite> for Type {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Sqlite>) -> diesel::serialize::Result {
        let s = match self {
            Type::Income => "income",
            Type::Expense => "expense",
            Type::Transfer => "transfer",
        };
        ToSql::<Text, Sqlite>::to_sql(s, out)
    }
}

impl FromSql<Text, Sqlite> for Type {
    fn from_sql(
        bytes: <Sqlite as diesel::backend::Backend>::RawValue<'_>,
    ) -> diesel::deserialize::Result<Self> {
        let s = <String as FromSql<Text, Sqlite>>::from_sql(bytes)?;
        match s.as_str() {
            "income" | "Income" => Ok(Type::Income),
            "expense" | "Expense" => Ok(Type::Expense),
            "transfer" | "Transfer" => Ok(Type::Transfer),
            _ => Err(format!("Unknown transaction type: {s}").into()),
        }
    }
}

pub fn normalize_payment_method(raw: &str) -> Result<&'static str, String> {
    match raw.trim().to_lowercase().as_str() {
        "debit" | "débito" | "debito" => Ok("debit"),
        "pix" => Ok("pix"),
        "cash" | "dinheiro" => Ok("cash"),
        "boleto" => Ok("boleto"),
        "credit" | "crédito" | "credito" => Ok("credit"),
        "salary_account" | "conta salário" | "conta salario" => Ok("salary_account"),
        "transfer" | "transferência" | "transferencia" => Ok("transfer"),
        other => Err(format!("Forma de pagamento inválida: '{other}'")),
    }
}

pub fn normalize_account_type(raw: &str) -> Result<&'static str, String> {
    match raw.trim().to_lowercase().as_str() {
        "checking" | "corrente" => Ok("checking"),
        "savings" | "poupanca" | "poupança" | "reserva" => Ok("savings"),
        "investment" | "investimento" | "investimentos" => Ok("investment"),
        "cash" | "dinheiro" | "carteira" => Ok("cash"),
        "credit_card" | "cartao" | "cartão" => Ok("credit_card"),
        other => Err(format!("Tipo de conta inválido: '{other}'")),
    }
}

pub fn normalize_transfer_kind(raw: &str) -> Result<&'static str, String> {
    match raw.trim().to_lowercase().as_str() {
        "general" => Ok("general"),
        "investment" | "investimento" => Ok("investment"),
        "savings" | "reserva" => Ok("savings"),
        "invoice_payment" | "fatura" => Ok("invoice_payment"),
        other => Err(format!("Tipo de transferência inválido: '{other}'")),
    }
}

/// Option 3 (Hybrid Simple): credit purchases do NOT deduct from the checking account balance
/// on purchase day, and informational rows (e.g. historical 0.00 entries) have zero balance impact.
pub fn transaction_balance_delta(
    tx_type: &Type,
    payment_method: &str,
    is_informational: bool,
    amount: i64,
) -> i64 {
    if is_informational {
        return 0;
    }
    match tx_type {
        Type::Income => amount,
        Type::Expense if payment_method == "credit" => 0,
        Type::Expense => -amount,
        Type::Transfer => 0,
    }
}

// ─────────────────────────────── DB ENTITIES ───────────────────────────────

#[derive(Clone, Debug, Queryable, Insertable, Selectable)]
#[diesel(table_name = crate::schema::users)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct User {
    pub id: String,
    pub name: String,
    pub currency: Option<String>,
    pub email: String,
    pub password: Option<String>,
    pub cycle_start_day: i32,
    pub default_savings_percent: f32,
    pub deleted_at: Option<PrimitiveDateTime>,
    pub created_at: PrimitiveDateTime,
    pub updated_at: PrimitiveDateTime,
}

#[derive(Serialize, Clone, Debug)]
pub struct UserResponse {
    pub id: String,
    pub name: String,
    pub currency: String,
    pub email: String,
    pub cycle_start_day: i32,
    pub default_savings_percent: f32,
    #[serde(with = "flexible_datetime")]
    pub created_at: PrimitiveDateTime,
    #[serde(with = "flexible_datetime")]
    pub updated_at: PrimitiveDateTime,
}

impl From<User> for UserResponse {
    fn from(u: User) -> Self {
        Self {
            id: u.id,
            name: u.name,
            currency: u.currency.unwrap_or_else(|| "BRL".to_string()),
            email: u.email,
            cycle_start_day: u.cycle_start_day,
            default_savings_percent: u.default_savings_percent,
            created_at: u.created_at,
            updated_at: u.updated_at,
        }
    }
}

#[derive(Deserialize, Debug)]
pub struct UpdateMeRequest {
    pub name: Option<String>,
    pub currency: Option<String>,
    pub cycle_start_day: Option<i32>,
    pub default_savings_percent: Option<f32>,
}

#[derive(Deserialize, Debug)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

#[derive(Serialize, Clone, Debug, Queryable, Insertable, Selectable)]
#[diesel(table_name = crate::schema::accounts)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct Account {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
    pub balance: i64,
    pub bank_name: String,
    pub account_type: String,
    pub user_id: String,
    #[serde(with = "flexible_datetime::option")]
    pub deleted_at: Option<PrimitiveDateTime>,
    #[serde(with = "flexible_datetime")]
    pub created_at: PrimitiveDateTime,
    #[serde(with = "flexible_datetime")]
    pub updated_at: PrimitiveDateTime,
}

#[derive(Deserialize, Debug)]
pub struct CreateAccountRequest {
    pub name: String,
    pub icon: Option<String>,
    pub balance: Option<i64>,
    pub bank_name: String,
    pub account_type: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct UpdateAccountRequest {
    pub name: Option<String>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub icon: Option<Option<String>>,
    pub bank_name: Option<String>,
    pub account_type: Option<String>,
    pub balance: Option<i64>,
}

#[derive(Serialize, Clone, Debug, Queryable, Insertable, Selectable)]
#[diesel(table_name = crate::schema::categories)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct Category {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub color: Option<String>,
    pub icon: Option<String>,
    pub category_type: String,
    pub is_fixed: bool,
    pub is_salary: bool,
    pub user_id: String,
    #[serde(with = "flexible_datetime::option")]
    pub deleted_at: Option<PrimitiveDateTime>,
    #[serde(with = "flexible_datetime")]
    pub created_at: PrimitiveDateTime,
    #[serde(with = "flexible_datetime")]
    pub updated_at: PrimitiveDateTime,
}

#[derive(Deserialize, Debug)]
pub struct CreateCategoryRequest {
    pub name: String,
    pub description: Option<String>,
    pub color: Option<String>,
    pub icon: Option<String>,
    pub category_type: Option<String>,
    pub is_fixed: Option<bool>,
    pub is_salary: Option<bool>,
}

#[derive(Deserialize, Debug)]
pub struct UpdateCategoryRequest {
    pub name: Option<String>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub description: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub color: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub icon: Option<Option<String>>,
    pub category_type: Option<String>,
    pub is_fixed: Option<bool>,
    pub is_salary: Option<bool>,
}

#[derive(Serialize, Clone, Debug, Queryable, Insertable, Selectable)]
#[diesel(table_name = crate::schema::tags)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct Tag {
    pub id: String,
    pub name: String,
    pub color: Option<String>,
    pub user_id: String,
    #[serde(with = "flexible_datetime::option")]
    pub deleted_at: Option<PrimitiveDateTime>,
    #[serde(with = "flexible_datetime")]
    pub created_at: PrimitiveDateTime,
    #[serde(with = "flexible_datetime")]
    pub updated_at: PrimitiveDateTime,
}

#[derive(Serialize, Clone, Debug)]
pub struct TagResponse {
    pub id: String,
    pub name: String,
    pub color: Option<String>,
}

impl From<Tag> for TagResponse {
    fn from(t: Tag) -> Self {
        Self {
            id: t.id,
            name: t.name,
            color: t.color,
        }
    }
}

#[derive(Deserialize, Debug)]
pub struct CreateTagRequest {
    pub name: String,
    pub color: Option<String>,
}

#[derive(Clone, Debug, Queryable, Insertable, Selectable)]
#[diesel(table_name = crate::schema::transactions)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct Transaction {
    pub id: String,
    pub amount: i64,
    pub description: Option<String>,
    pub date: PrimitiveDateTime,
    pub type_: Type,
    pub payment_method: String,
    pub card_name: Option<String>,
    pub account_id: String,
    pub transfer_id: Option<String>,
    pub category_id: Option<String>,
    pub is_fixed: Option<bool>,
    pub is_informational: bool,
    pub credit_paid: bool,
    pub installment_group_id: Option<String>,
    pub installment_number: Option<i32>,
    pub installment_total: Option<i32>,
    pub recurring_rule_id: Option<String>,
    pub needs_confirmation: bool,
    pub user_id: String,
    pub deleted_at: Option<PrimitiveDateTime>,
    pub created_at: PrimitiveDateTime,
    pub updated_at: PrimitiveDateTime,
}

#[derive(AsChangeset)]
#[diesel(table_name = crate::schema::transactions)]
#[diesel(treat_none_as_null = true)]
pub struct TransactionUpdateChangeset {
    pub amount: i64,
    pub description: Option<String>,
    pub date: PrimitiveDateTime,
    pub type_: Type,
    pub payment_method: String,
    pub card_name: Option<String>,
    pub account_id: String,
    pub category_id: Option<String>,
    pub is_fixed: Option<bool>,
    pub is_informational: bool,
    pub credit_paid: bool,
    pub needs_confirmation: bool,
    pub updated_at: PrimitiveDateTime,
}

#[derive(Serialize, Clone, Debug)]
pub struct TransactionResponse {
    pub id: String,
    pub amount: i64,
    pub description: Option<String>,
    #[serde(with = "flexible_datetime")]
    pub date: PrimitiveDateTime,
    #[serde(rename = "type")]
    pub type_: Type,
    pub payment_method: String,
    pub card_name: Option<String>,
    pub account_id: String,
    pub transfer_id: Option<String>,
    pub category_id: Option<String>,
    pub is_fixed: Option<bool>,
    pub is_informational: bool,
    pub credit_paid: bool,
    pub installment_group_id: Option<String>,
    pub installment_number: Option<i32>,
    pub installment_total: Option<i32>,
    pub recurring_rule_id: Option<String>,
    pub needs_confirmation: bool,
    pub user_id: String,
    pub tags: Vec<TagResponse>,
    #[serde(with = "flexible_datetime")]
    pub created_at: PrimitiveDateTime,
    #[serde(with = "flexible_datetime")]
    pub updated_at: PrimitiveDateTime,
}

impl TransactionResponse {
    pub fn from_row(row: Transaction, tags: Vec<TagResponse>) -> Self {
        Self {
            id: row.id,
            amount: row.amount,
            description: row.description,
            date: row.date,
            type_: row.type_,
            payment_method: row.payment_method,
            card_name: row.card_name,
            account_id: row.account_id,
            transfer_id: row.transfer_id,
            category_id: row.category_id,
            is_fixed: row.is_fixed,
            is_informational: row.is_informational,
            credit_paid: row.credit_paid,
            installment_group_id: row.installment_group_id,
            installment_number: row.installment_number,
            installment_total: row.installment_total,
            recurring_rule_id: row.recurring_rule_id,
            needs_confirmation: row.needs_confirmation,
            user_id: row.user_id,
            tags,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

#[derive(Deserialize, Debug)]
pub struct CreateTransactionRequest {
    pub amount: i64,
    pub description: Option<String>,
    #[serde(default, with = "flexible_datetime::option")]
    pub date: Option<PrimitiveDateTime>,
    #[serde(rename = "type")]
    pub type_: Type,
    pub payment_method: Option<String>,
    pub card_name: Option<String>,
    pub account_id: String,
    pub category_id: Option<String>,
    pub is_fixed: Option<bool>,
    pub is_informational: Option<bool>,
    pub installment_total: Option<i32>,
    pub needs_confirmation: Option<bool>,
    pub tag_ids: Option<Vec<String>>,
}

#[derive(Deserialize, Debug)]
pub struct UpdateTransactionRequest {
    pub amount: Option<i64>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub description: Option<Option<String>>,
    #[serde(default, with = "flexible_datetime::option")]
    pub date: Option<PrimitiveDateTime>,
    #[serde(rename = "type")]
    pub type_: Option<Type>,
    pub payment_method: Option<String>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub card_name: Option<Option<String>>,
    pub account_id: Option<String>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub category_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub is_fixed: Option<Option<bool>>,
    pub is_informational: Option<bool>,
    pub credit_paid: Option<bool>,
    pub needs_confirmation: Option<bool>,
    pub tag_ids: Option<Vec<String>>,
}

#[derive(Clone, Debug, Queryable, Insertable, Selectable)]
#[diesel(table_name = crate::schema::transaction_tags)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct TransactionTag {
    pub transaction_id: String,
    pub tag_id: String,
}

#[derive(Clone, Debug, Queryable, Insertable, Selectable)]
#[diesel(table_name = crate::schema::account_transfers)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct AccountTransfer {
    pub id: String,
    pub from_account_id: String,
    pub to_account_id: Option<String>,
    pub amount: i64,
    pub description: Option<String>,
    pub date: PrimitiveDateTime,
    pub transfer_kind: String,
    pub user_id: String,
    pub deleted_at: Option<PrimitiveDateTime>,
    pub created_at: PrimitiveDateTime,
    pub updated_at: PrimitiveDateTime,
}

#[derive(Serialize, Clone, Debug)]
pub struct TransferResponse {
    pub id: String,
    pub from_account_id: String,
    pub to_account_id: Option<String>,
    pub amount: i64,
    pub description: Option<String>,
    #[serde(with = "flexible_datetime")]
    pub date: PrimitiveDateTime,
    pub transfer_kind: String,
    pub user_id: String,
    pub tags: Vec<TagResponse>,
    #[serde(with = "flexible_datetime")]
    pub created_at: PrimitiveDateTime,
    #[serde(with = "flexible_datetime")]
    pub updated_at: PrimitiveDateTime,
}

impl TransferResponse {
    pub fn from_row(row: AccountTransfer, tags: Vec<TagResponse>) -> Self {
        Self {
            id: row.id,
            from_account_id: row.from_account_id,
            to_account_id: row.to_account_id,
            amount: row.amount,
            description: row.description,
            date: row.date,
            transfer_kind: row.transfer_kind,
            user_id: row.user_id,
            tags,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

#[derive(Deserialize, Debug)]
pub struct CreateTransferRequest {
    pub from_account_id: String,
    pub to_account_id: Option<String>,
    pub amount: i64,
    pub description: Option<String>,
    #[serde(default, with = "flexible_datetime::option")]
    pub date: Option<PrimitiveDateTime>,
    pub transfer_kind: Option<String>,
    pub tag_ids: Option<Vec<String>>,
}

#[derive(Deserialize, Debug)]
pub struct PayCreditInvoiceRequest {
    pub card_name: String,
    pub from_account_id: String,
    pub amount: i64,
    #[serde(default, with = "flexible_datetime::option")]
    pub date: Option<PrimitiveDateTime>,
    pub description: Option<String>,
    pub transaction_ids: Option<Vec<String>>,
}

#[derive(Clone, Debug, Queryable, Insertable, Selectable)]
#[diesel(table_name = crate::schema::transfer_tags)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct TransferTag {
    pub transfer_id: String,
    pub tag_id: String,
}

#[derive(Serialize, Clone, Debug, Queryable, Insertable, Selectable)]
#[diesel(table_name = crate::schema::cycle_configs)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct CycleConfig {
    pub id: String,
    pub user_id: String,
    pub cycle_key: String,
    pub start_date: String,
    pub end_date: String,
    pub label: Option<String>,
    pub savings_mode: String,
    pub savings_percent: Option<f32>,
    pub savings_fixed_amount: Option<i64>,
    pub notes: Option<String>,
    #[serde(with = "flexible_datetime")]
    pub created_at: PrimitiveDateTime,
    #[serde(with = "flexible_datetime")]
    pub updated_at: PrimitiveDateTime,
}

#[derive(Deserialize, Debug)]
pub struct UpsertCycleConfigRequest {
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub label: Option<Option<String>>,
    pub savings_mode: String,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub savings_percent: Option<Option<f32>>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub savings_fixed_amount: Option<Option<i64>>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub notes: Option<Option<String>>,
}

#[derive(Serialize, Clone, Debug, Queryable, Insertable, Selectable)]
#[diesel(table_name = crate::schema::recurring_rules)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct RecurringRule {
    pub id: String,
    pub user_id: String,
    pub description: String,
    pub default_amount: i64,
    #[serde(rename = "type")]
    pub type_: Type,
    pub payment_method: String,
    pub account_id: String,
    pub category_id: Option<String>,
    pub day_of_month: i32,
    pub is_variable_amount: bool,
    pub is_fixed: bool,
    pub active: bool,
    #[serde(with = "flexible_datetime::option")]
    pub deleted_at: Option<PrimitiveDateTime>,
    #[serde(with = "flexible_datetime")]
    pub created_at: PrimitiveDateTime,
    #[serde(with = "flexible_datetime")]
    pub updated_at: PrimitiveDateTime,
}

#[derive(Deserialize, Debug)]
pub struct CreateRecurringRuleRequest {
    pub description: String,
    pub default_amount: i64,
    #[serde(rename = "type")]
    pub type_: Option<Type>,
    pub payment_method: Option<String>,
    pub account_id: String,
    pub category_id: Option<String>,
    pub day_of_month: i32,
    pub is_variable_amount: Option<bool>,
    pub is_fixed: Option<bool>,
    pub active: Option<bool>,
}

#[derive(Deserialize, Debug)]
pub struct UpdateRecurringRuleRequest {
    pub description: Option<String>,
    pub default_amount: Option<i64>,
    #[serde(rename = "type")]
    pub type_: Option<Type>,
    pub payment_method: Option<String>,
    pub account_id: Option<String>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub category_id: Option<Option<String>>,
    pub day_of_month: Option<i32>,
    pub is_variable_amount: Option<bool>,
    pub is_fixed: Option<bool>,
    pub active: Option<bool>,
}

#[derive(Deserialize, Debug)]
pub struct GenerateRecurringRequest {
    pub cycle_key: String,
}
