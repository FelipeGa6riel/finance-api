use actix_web::{web, App, HttpResponse, HttpServer, Responder, get, post};
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqlitePoolOptions, sqlite::SqliteRow, Row, SqlitePool};
use time::PrimitiveDateTime;
use dotenvy::dotenv;
use std::env;

time::serde::format_description!(date_format, PrimitiveDateTime, "[year]-[month]-[day] [hour]:[minute]:[second]");

fn default_datetime() -> PrimitiveDateTime {
    let now = time::OffsetDateTime::now_utc();
    PrimitiveDateTime::new(now.date(), now.time())
}

#[derive(Serialize, Deserialize, Clone)]
struct User {
    id: i32,
    name: String,
    email: String,
    #[serde(with = "date_format", default = "default_datetime")]
    created_at: PrimitiveDateTime,
    #[serde(with = "date_format", default = "default_datetime")]
    updated_at: PrimitiveDateTime,
}

#[derive(Serialize, Deserialize, Clone)]
struct Category {
    id: i32,
    name: String,
    description: String,
    user: Option<User>,
    #[serde(with = "date_format", default = "default_datetime")]
    created_at: PrimitiveDateTime,
    #[serde(with = "date_format", default = "default_datetime")]
    updated_at: PrimitiveDateTime,
}

#[derive(Serialize, Deserialize, Clone)]
struct Account {
    id: i32,
    user: User,
    name: String,
    bank_name: String,
    balance: f64,
    transactions: Vec<Transaction>,
    #[serde(with = "date_format", default = "default_datetime")]
    created_at: PrimitiveDateTime,
    #[serde(with = "date_format", default = "default_datetime")]
    updated_at: PrimitiveDateTime,
}

#[derive(Serialize, Deserialize, Clone, sqlx::Type)]
#[sqlx(rename_all = "lowercase")]
enum Type {
    Income,
    Expense,
    Transfer,
}

#[derive(Serialize, Deserialize, Clone)]
struct Transaction {
    id: i32,
    user: User,
    r#type: Type,
    amount: f64,
    description: Option<String>,
    category: Category,
    account: Option<Account>,
    #[serde(with = "date_format", default = "default_datetime")]
    created_at: PrimitiveDateTime,
    #[serde(with = "date_format", default = "default_datetime")]
    updated_at: PrimitiveDateTime,
}

fn map_transaction(row: &SqliteRow) -> Result<Transaction, sqlx::Error> {
    let user = User {
        id: row.try_get("user_id")?,
        name: row.try_get("user_name")?,
        email: row.try_get("user_email")?,
        created_at: row.try_get("user_created_at")?,
        updated_at: row.try_get("user_updated_at")?,
    };

    let category = Category {
        id: row.try_get("category_id")?,
        name: row.try_get("category_name")?,
        description: row.try_get("category_description")?,
        user: None,
        created_at: row.try_get("category_created_at")?,
        updated_at: row.try_get("category_updated_at")?,
    };

    Ok(Transaction {
        id: row.try_get("id")?,
        user,
        r#type: row.try_get("type")?,
        amount: row.try_get("amount")?,
        description: row.try_get("description")?,
        category,
        account: None,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn map_account(row: &SqliteRow) -> Result<Account, sqlx::Error> {
    let user = User {
        id: row.try_get("user_id")?,
        name: row.try_get("user_name")?,
        email: row.try_get("user_email")?,
        created_at: row.try_get("user_created_at")?,
        updated_at: row.try_get("user_updated_at")?,
    };

    Ok(Account {
        id: row.try_get("id")?,
        user,
        name: row.try_get("name")?,
        bank_name: row.try_get("bank_name")?,
        balance: row.try_get("balance")?,
        transactions: Vec::new(),
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

#[get("/transactions")]
async fn get_transactions(pool: web::Data<SqlitePool>) -> impl Responder {
    let query = r#"
        SELECT 
            t.id, t.type, t.amount, t.description, t.account_id, t.created_at, t.updated_at,
            u.id as user_id, u.name as user_name, u.email as user_email, u.created_at as user_created_at, u.updated_at as user_updated_at,
            c.id as category_id, c.name as category_name, c.description as category_description, c.created_at as category_created_at, c.updated_at as category_updated_at
        FROM transactions t
        JOIN users u ON t.user_id = u.id
        JOIN categories c ON t.category_id = c.id
    "#;

    let result = sqlx::query(query)
        .try_map(|row: SqliteRow| map_transaction(&row))
        .fetch_all(pool.get_ref())
        .await;

    match result {
        Ok(transactions) => HttpResponse::Ok().json(transactions),
        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
    }
}

#[get("/transactions/{id}")]
async fn get_transaction(id: web::Path<i32>, pool: web::Data<SqlitePool>) -> impl Responder {
    let query = r#"
        SELECT 
            t.id, t.type, t.amount, t.description, t.account_id, t.created_at, t.updated_at,
            u.id as user_id, u.name as user_name, u.email as user_email, u.created_at as user_created_at, u.updated_at as user_updated_at,
            c.id as category_id, c.name as category_name, c.description as category_description, c.created_at as category_created_at, c.updated_at as category_updated_at
        FROM transactions t
        JOIN users u ON t.user_id = u.id
        JOIN categories c ON t.category_id = c.id
        WHERE t.id = ?
    "#;

    let result = sqlx::query(query)
        .bind(id.into_inner())
        .try_map(|row: SqliteRow| map_transaction(&row))
        .fetch_optional(pool.get_ref())
        .await;

    match result {
        Ok(Some(transaction)) => HttpResponse::Ok().json(transaction),
        Ok(None) => HttpResponse::NotFound().finish(),
        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
    }
}

#[post("/transaction")]
async fn create_transaction(
    transaction: web::Json<Transaction>,
    pool: web::Data<SqlitePool>,
) -> impl Responder {
    let t = transaction.into_inner();

    let format = time::format_description::parse("[year]-[month]-[day] [hour]:[minute]:[second]").unwrap();
    
    let result = sqlx::query(
        "INSERT INTO transactions (id, user_id, type, amount, description, category_id, account_id, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(t.id)
    .bind(t.user.id)
    .bind(&t.r#type)
    .bind(t.amount)
    .bind(&t.description)
    .bind(t.category.id)
    .bind(t.account.as_ref().map(|a| a.id).unwrap_or(1))
    .bind(time::OffsetDateTime::now_utc().format(&format).unwrap())
    .bind(time::OffsetDateTime::now_utc().format(&format).unwrap())
    .execute(pool.get_ref())
    .await;

    match result {
        Ok(_) => HttpResponse::Created().json(t),
        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
    }
}

#[post("/account")]
async fn create_account(
    account: web::Json<Account>,
    pool: web::Data<SqlitePool>,
) -> impl Responder {
    let a = account.into_inner();
    let result = sqlx::query(
        "INSERT INTO accounts (id, user_id, name, bank_name, balance, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(a.id)
    .bind(a.user.id)
    .bind(&a.name)
    .bind(&a.bank_name)
    .bind(a.balance)
    .bind(a.created_at)
    .bind(a.updated_at)
    .execute(pool.get_ref())
    .await;

    match result {
        Ok(_) => HttpResponse::Created().json(a),
        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
    }
}


#[post("/user")]
async fn create_user(
    user: web::Json<User>,
    pool: web::Data<SqlitePool>,
) -> impl Responder {
    let u = user.into_inner();
    let result = sqlx::query(
        "INSERT INTO users (id, name, email, created_at, updated_at) VALUES (?, ?, ?, ?, ?)"
    )
    .bind(u.id)
    .bind(&u.name)
    .bind(&u.email)
    .bind(u.created_at)
    .bind(u.updated_at)
    .execute(pool.get_ref())
    .await;

    match result {
        Ok(_) => HttpResponse::Created().json(u),
        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
    }
}

#[post("/category")]
async fn create_category(
    category: web::Json<Category>,
    pool: web::Data<SqlitePool>,
) -> impl Responder {
    let c = category.into_inner();
    let result = sqlx::query(
        "INSERT INTO categories (id, name, description, created_at, updated_at) VALUES (?, ?, ?, ?, ?)"
    )
    .bind(c.id)
    .bind(&c.name)
    .bind(&c.description)
    .bind(c.created_at)
    .bind(c.updated_at)
    .execute(pool.get_ref())
    .await;

    match result {
        Ok(_) => HttpResponse::Created().json(c),
        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
    }
}

#[get("/accounts/{id}")]
async fn get_account(id: web::Path<i32>, pool: web::Data<SqlitePool>) -> impl Responder {
    let query = r#"
        SELECT 
            a.id, a.name, a.bank_name, a.balance, a.created_at, a.updated_at,
            u.id as user_id, u.name as user_name, u.email as user_email, u.created_at as user_created_at, u.updated_at as user_updated_at
        FROM accounts a
        JOIN users u ON a.user_id = u.id
        WHERE a.id = ?
    "#;
    let result = sqlx::query(query)
        .bind(id.into_inner())
        .try_map(|row: SqliteRow| map_account(&row))
        .fetch_optional(pool.get_ref())
        .await;

    match result {
        Ok(Some(mut account)) => {
            let tx_query = r#"
                SELECT 
                    t.id, t.type, t.amount, t.description, t.account_id, t.created_at, t.updated_at,
                    u.id as user_id, u.name as user_name, u.email as user_email, u.created_at as user_created_at, u.updated_at as user_updated_at,
                    c.id as category_id, c.name as category_name, c.description as category_description, c.created_at as category_created_at, c.updated_at as category_updated_at
                FROM transactions t
                JOIN users u ON t.user_id = u.id
                JOIN categories c ON t.category_id = c.id
                WHERE t.account_id = ?
            "#;
            if let Ok(transactions) = sqlx::query(tx_query)
                .bind(account.id)
                .try_map(|row: SqliteRow| map_transaction(&row))
                .fetch_all(pool.get_ref())
                .await
            {
                account.transactions = transactions;
            }
            HttpResponse::Ok().json(account)
        },
        Ok(None) => HttpResponse::NotFound().finish(),
        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
    }
}

#[get("/accounts")]
async fn get_accounts(
    pool: web::Data<SqlitePool>,
) -> impl Responder {
    let query = r#"
        SELECT 
            a.id, a.name, a.bank_name, a.balance, a.created_at, a.updated_at,
            u.id as user_id, u.name as user_name, u.email as user_email, u.created_at as user_created_at, u.updated_at as user_updated_at
        FROM accounts a
        JOIN users u ON a.user_id = u.id
    "#;

    let result = sqlx::query(query)
        .try_map(|row: SqliteRow| map_account(&row))
        .fetch_all(pool.get_ref())
        .await;

    match result {
        Ok(mut accounts) => {
            let tx_query = r#"
                SELECT 
                    t.id, t.type, t.amount, t.description, t.account_id, t.created_at, t.updated_at,
                    u.id as user_id, u.name as user_name, u.email as user_email, u.created_at as user_created_at, u.updated_at as user_updated_at,
                    c.id as category_id, c.name as category_name, c.description as category_description, c.created_at as category_created_at, c.updated_at as category_updated_at
                FROM transactions t
                JOIN users u ON t.user_id = u.id
                JOIN categories c ON t.category_id = c.id
                WHERE t.account_id = ?
            "#;
            for account in &mut accounts {
                if let Ok(transactions) = sqlx::query(tx_query)
                    .bind(account.id)
                    .try_map(|row: SqliteRow| map_transaction(&row))
                    .fetch_all(pool.get_ref())
                    .await
                {
                    account.transactions = transactions;
                }
            }
            HttpResponse::Ok().json(accounts)
        },
        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
    }
}

async fn setup_db(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS users (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            email TEXT NOT NULL,
            created_at DATETIME NOT NULL,
            updated_at DATETIME NOT NULL
        );
        "#
    ).execute(pool).await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS categories (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            description TEXT NOT NULL,
            created_at DATETIME NOT NULL,
            updated_at DATETIME NOT NULL
        );
        "#
    ).execute(pool).await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS accounts (
            id INTEGER PRIMARY KEY,
            user_id INTEGER NOT NULL,
            name TEXT NOT NULL,
            bank_name TEXT NOT NULL,
            balance REAL NOT NULL,
            created_at DATETIME NOT NULL,
            updated_at DATETIME NOT NULL,
            FOREIGN KEY(user_id) REFERENCES users(id)
        );
        "#
    ).execute(pool).await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS transactions (
            id INTEGER PRIMARY KEY,
            user_id INTEGER NOT NULL,
            amount REAL NOT NULL,
            description TEXT NOT NULL,
            type TEXT NOT NULL,
            category_id INTEGER NOT NULL,
            account_id INTEGER NOT NULL,
            created_at DATETIME NOT NULL,
            updated_at DATETIME NOT NULL,
            FOREIGN KEY(user_id) REFERENCES users(id),
            FOREIGN KEY(category_id) REFERENCES categories(id),
            FOREIGN KEY(account_id) REFERENCES accounts(id)
        );
        "#
    ).execute(pool).await?;

    // Seed
    let u_count: (i64,) = sqlx::query_as("SELECT count(*) FROM users").fetch_one(pool).await?;
    if u_count.0 == 0 {
        sqlx::query("INSERT INTO users (id, name, email, created_at, updated_at) VALUES (1, 'John Doe', 'john.doe@example.com', '2023-01-01 00:00:00', '2023-01-01 00:00:00'), (2, 'Jane Smith', 'jane.smith@example.com', '2023-01-01 00:00:00', '2023-01-01 00:00:00')")
            .execute(pool).await?;
        sqlx::query("INSERT INTO categories (id, name, description, created_at, updated_at) VALUES (1, 'Food', 'Expenses on food and dining', '2023-01-01 00:00:00', '2023-01-01 00:00:00'), (2, 'Transport', 'Expenses on transportation', '2023-01-01 00:00:00', '2023-01-01 00:00:00')")
            .execute(pool).await?;
        sqlx::query("INSERT INTO accounts (id, user_id, name, bank_name, balance, created_at, updated_at) VALUES (1, 1, 'Current Account', 'Nubank', 3500.0, '2023-01-01 00:00:00', '2023-01-01 00:00:00'), (2, 2, 'Savings', 'Inter', 5000.0, '2023-01-01 00:00:00', '2023-01-01 00:00:00')")
            .execute(pool).await?;
        sqlx::query("INSERT INTO transactions (id, user_id, amount, description, type, category_id, account_id, created_at, updated_at) VALUES 
            (1, 1, 100.0, 'Grocery shopping', 'expense', 1, 1, '2023-01-01 00:00:00', '2023-01-01 00:00:00'),
            (2, 1, 50.0, 'Lunch', 'expense', 1, 1, '2023-01-02 00:00:00', '2023-01-02 00:00:00'),
            (3, 1, 20.0, 'Coffee', 'expense', 1, 1, '2023-01-03 00:00:00', '2023-01-03 00:00:00'),
            (4, 1, 15.0, 'Bus ticket', 'expense', 2, 1, '2023-01-04 00:00:00', '2023-01-04 00:00:00'),
            (5, 1, 2000.0, 'Salary', 'income', 1, 1, '2023-01-05 00:00:00', '2023-01-05 00:00:00'),
            (6, 1, 150.0, 'Dinner', 'expense', 1, 1, '2023-01-06 00:00:00', '2023-01-06 00:00:00'),
            (7, 1, 30.0, 'Uber', 'expense', 2, 1, '2023-01-07 00:00:00', '2023-01-07 00:00:00'),
            (8, 2, 50.0, 'Taxi ride', 'expense', 2, 2, '2023-01-01 00:00:00', '2023-01-01 00:00:00'),
            (9, 2, 25.0, 'Breakfast', 'expense', 1, 2, '2023-01-02 00:00:00', '2023-01-02 00:00:00'),
            (10, 2, 10.0, 'Subway', 'expense', 2, 2, '2023-01-03 00:00:00', '2023-01-03 00:00:00'),
            (11, 2, 3000.0, 'Freelance', 'income', 1, 2, '2023-01-04 00:00:00', '2023-01-04 00:00:00'),
            (12, 2, 80.0, 'Groceries', 'expense', 1, 2, '2023-01-05 00:00:00', '2023-01-05 00:00:00'),
            (13, 2, 45.0, 'Gas', 'expense', 2, 2, '2023-01-06 00:00:00', '2023-01-06 00:00:00'),
            (14, 2, 120.0, 'Restaurant', 'expense', 1, 2, '2023-01-07 00:00:00', '2023-01-07 00:00:00')")
            .execute(pool).await?;
    }

    Ok(())
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv().ok();

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:sqlite3.db".into()))
        .await
        .expect("Failed to create pool");

    setup_db(&pool).await.expect("Failed to initialize database");

    let app_data = web::Data::new(pool);

    HttpServer::new(move || {
        App::new()
            .app_data(app_data.clone())
            .service(get_accounts)
            .service(get_transactions)
            .service(get_transaction)
            .service(get_account)
            .service(create_user)
            .service(create_category)
            .service(create_account)
            .service(create_transaction)
    })
    .bind(("localhost", 8007))?
    .run()
    .await
}
