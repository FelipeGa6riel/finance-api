use actix_web::{web, App, HttpResponse, HttpServer, Responder, get, post};
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqlitePoolOptions, sqlite::SqliteRow, Row, SqlitePool};
use time::PrimitiveDateTime;

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
struct Transaction {
    id: i32,
    user: User,
    amount: f64,
    description: Option<String>,
    category: Category,
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
        amount: row.try_get("amount")?,
        description: row.try_get("description")?,
        category,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

#[get("/transactions")]
async fn get_transactions(pool: web::Data<SqlitePool>) -> impl Responder {
    let query = r#"
        SELECT 
            t.id, t.amount, t.created_at, t.updated_at,
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
            t.id, t.amount, t.created_at, t.updated_at,
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
        "INSERT INTO transactions (id, user_id, amount, category_id, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)"
    )
    .bind(t.id)
    .bind(t.user.id)
    .bind(t.amount)
    .bind(t.category.id)
    .bind(time::OffsetDateTime::now_utc().format(&format).unwrap())
    .bind(time::OffsetDateTime::now_utc().format(&format).unwrap())
    .execute(pool.get_ref())
    .await;

    match result {
        Ok(_) => HttpResponse::Created().json(t),
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
        CREATE TABLE IF NOT EXISTS transactions (
            id INTEGER PRIMARY KEY,
            user_id INTEGER NOT NULL,
            amount REAL NOT NULL,
            category_id INTEGER NOT NULL,
            created_at DATETIME NOT NULL,
            updated_at DATETIME NOT NULL,
            FOREIGN KEY(user_id) REFERENCES users(id),
            FOREIGN KEY(category_id) REFERENCES categories(id)
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
        sqlx::query("INSERT INTO transactions (id, user_id, amount, category_id, created_at, updated_at) VALUES (1, 1, 100.0, 1, '2023-01-01 00:00:00', '2023-01-01 00:00:00'), (2, 2, 50.0, 2, '2023-01-01 00:00:00', '2023-01-01 00:00:00')")
            .execute(pool).await?;
    }

    Ok(())
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect("sqlite:sqlite3.db")
        .await
        .expect("Failed to create pool");

    setup_db(&pool).await.expect("Failed to initialize database");

    let app_data = web::Data::new(pool);

    HttpServer::new(move || {
        App::new()
            .app_data(app_data.clone())
            .service(get_transactions)
            .service(get_transaction)
            .service(create_transaction)
            .service(create_user)
            .service(create_category)
    })
    .bind(("localhost", 8007))?
    .run()
    .await
}
