use diesel::deserialize::FromSql;
use diesel::sql_types::Text;
use diesel::sqlite::Sqlite;
use serde::{Deserialize, Serialize};
use time::PrimitiveDateTime;
use diesel::prelude::*;
use diesel::serialize::{ToSql, Output};

time::serde::format_description!(
    date_format,
    PrimitiveDateTime,
    "[year]-[month]-[day] [hour]:[minute]:[second]"
);

fn default_datetime() -> PrimitiveDateTime {
    let now = time::OffsetDateTime::now_utc();
    PrimitiveDateTime::new(now.date(), now.time())
}

#[derive(Serialize, Deserialize, Clone, Queryable, Insertable)]
#[diesel(table_name = crate::schema::users)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct User {
    pub id: i32,
    pub name: String,
    pub email: String,
    #[serde(with = "date_format", default = "default_datetime")]
    pub created_at: PrimitiveDateTime,
    #[serde(with = "date_format", default = "default_datetime")]
    pub updated_at: PrimitiveDateTime,
}

#[derive(Serialize, Deserialize, Clone, Queryable, Insertable)]
#[diesel(table_name = crate::schema::categories)]
#[diesel(belongs_to(User))]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct Category {
    pub id: i32,
    pub name: String,
    pub description: Option<String>,
    #[diesel()]
    pub user_id: Option<i32>,
    #[serde(with = "date_format", default = "default_datetime")]
    pub created_at: PrimitiveDateTime,
    #[serde(with = "date_format", default = "default_datetime")]
    pub updated_at: PrimitiveDateTime,
}

#[derive(Serialize, Deserialize, Clone, Queryable, Insertable)]
#[diesel(table_name = crate::schema::accounts)]
#[diesel(belongs_to(User))]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct Account {
    pub id: i32,
    pub name: String,
    pub balance: i64,
    pub bank_name: String,
    pub user_id: i32,
    #[serde(with = "date_format", default = "default_datetime")]
    pub created_at: PrimitiveDateTime,
    #[serde(with = "date_format", default = "default_datetime")]
    pub updated_at: PrimitiveDateTime,
}

#[derive(Serialize, Deserialize, Clone, Queryable, Insertable, Selectable)]
#[diesel(table_name = crate::schema::transactions)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct Transaction {
    pub id: i32,
    pub amount: i64,
    pub description: Option<String>,
    #[serde(with = "date_format", default = "default_datetime")]
    pub date: PrimitiveDateTime,
    pub type_: Type,
    pub account_id: i32,
    pub transfer_id: Option<i32>,
    pub category_id: Option<i32>,
    pub user_id: i32,
    #[serde(with = "date_format", default = "default_datetime")]
    pub created_at: PrimitiveDateTime,
    #[serde(with = "date_format", default = "default_datetime")]
    pub updated_at: PrimitiveDateTime,
}

#[derive(Serialize, Deserialize, Clone, Queryable, Insertable)]
#[diesel(table_name = crate::schema::account_transfers)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
#[diesel(belongs_to(Account, foreign_key = from_account_id))]
#[diesel(belongs_to(Account, foreign_key = to_account_id))]
pub struct AccountTransfer {
    pub id: i32,
    pub from_account_id: i32,
    pub to_account_id: i32,
    pub amount: i64,
    pub description: Option<String>,
    #[serde(with = "date_format", default = "default_datetime")]
    pub date: PrimitiveDateTime,
    pub user_id: i32,
    #[serde(with = "date_format", default = "default_datetime")]
    pub created_at: PrimitiveDateTime,
    #[serde(with = "date_format", default = "default_datetime")]
    pub updated_at: PrimitiveDateTime,
}

#[derive(Serialize, Deserialize, Clone, diesel::AsExpression, diesel::FromSqlRow, Debug)]
#[diesel(sql_type = diesel::sql_types::Text)]
pub enum Type {
    Income,
    Expense,
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
    fn from_sql(bytes: <Sqlite as diesel::backend::Backend>::RawValue<'_>) -> diesel::deserialize::Result<Self> {
        let s = <String as FromSql<Text, Sqlite>>::from_sql(bytes)?;
        match s.as_str() {
            "income" => Ok(Type::Income),
            "expense" => Ok(Type::Expense),
            "transfer" => Ok(Type::Transfer),
            _ => Err(format!("Unknown transaction type: {}", s).into()),
        }
    }
}