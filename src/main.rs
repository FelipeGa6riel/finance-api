use actix_web::{App, HttpResponse, HttpServer, Responder, get, post, web};
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, Pool};
use diesel::{QueryDsl, RunQueryDsl, SqliteConnection};
use dotenvy::dotenv;
use std::env;
type SqlitePool = Pool<ConnectionManager<SqliteConnection>>;

mod models;
mod schema;
use models::{Account, Category, Transaction, User};

use self::schema::{accounts, categories, transactions, users};

#[get("/transactions")]
async fn get_transactions(pool: web::Data<SqlitePool>) -> impl Responder {
    use self::schema::transactions::dsl::*;

    let mut conn = pool
        .get()
        .expect("Failed to get a connection from the pool");

    let results = transactions
        .load::<Transaction>(&mut conn)
        .expect("Error loading transactions");

    HttpResponse::Ok().json(results)
}

#[get("/transactions/{id}")]
async fn get_transaction(tx_id: web::Path<i32>, pool: web::Data<SqlitePool>) -> impl Responder {
    use self::schema::transactions::dsl::*;

    let mut conn = pool
        .get()
        .expect("Failed to get a connection from the pool");

    let result = transactions
        .filter(id.eq(tx_id.into_inner()))
        .first::<Transaction>(&mut conn)
        .optional()
        .expect("Error loading transaction");

    match result {
        Some(transaction) => HttpResponse::Ok().json(transaction),
        None => HttpResponse::NotFound().body("Transaction not found"),
    }
}

#[post("/transaction")]
async fn create_transaction(
    transaction: web::Json<Transaction>,
    pool: web::Data<SqlitePool>,
) -> impl Responder {
    let new_transaction = transaction.into_inner();

    let mut conn = pool
        .get()
        .expect("Failed to get a connection from the pool");

    diesel::insert_into(transactions::table)
        .values(&new_transaction)
        .execute(&mut conn)
        .expect("Error inserting new transaction");

    HttpResponse::Created().json(new_transaction)
}

#[post("/users")]
async fn create_user(user: web::Json<User>, pool: web::Data<SqlitePool>) -> impl Responder {
    let new_user = user.into_inner();

    let mut conn = pool
        .get()
        .expect("Failed to get a connection from the pool");

    diesel::insert_into(users::table)
        .values(&new_user)
        .execute(&mut conn)
        .expect("Error inserting new user");

    HttpResponse::Created().json(new_user)
}

#[get("/users/{id}")]
async fn get_user(user_id: web::Path<i32>, pool: web::Data<SqlitePool>) -> impl Responder {
    use self::schema::users::dsl::*;

    let mut conn = pool
        .get()
        .expect("Failed to get a connection from the pool");

    let result = users
        .filter(id.eq(user_id.into_inner()))
        .first::<User>(&mut conn)
        .optional()
        .expect("Error loading user");

    match result {
        Some(user) => HttpResponse::Ok().json(user),
        None => HttpResponse::NotFound().body("User not found"),
    }
}

#[get("/users")]
async fn get_users(pool: web::Data<SqlitePool>) -> impl Responder {
    use self::schema::users::dsl::*;

    let mut conn = pool
        .get()
        .expect("Failed to get a connection from the pool");

    let results = users.load::<User>(&mut conn).expect("Error loading users");

    HttpResponse::Ok().json(results)
}

#[post("/categories")]
async fn create_category(
    category: web::Json<Category>,
    pool: web::Data<SqlitePool>,
) -> impl Responder {
    let new_category = category.into_inner();

    let mut conn = pool
        .get()
        .expect("Failed to get a connection from the pool");

    diesel::insert_into(categories::table)
        .values(&new_category)
        .execute(&mut conn)
        .expect("Error inserting new category");

    HttpResponse::Created().json(new_category)
}

#[get("/categories")]
async fn get_categories(pool: web::Data<SqlitePool>) -> impl Responder {
    use self::schema::categories::dsl::*;

    let mut conn = pool
        .get()
        .expect("Failed to get a connection from the pool");

    let results = categories
        .load::<Category>(&mut conn)
        .expect("Error loading categories");

    HttpResponse::Ok().json(results)
}

#[post("/account")]
async fn create_account(
    account: web::Json<Account>,
    pool: web::Data<SqlitePool>,
) -> impl Responder {
    let new_account = account.into_inner();

    let mut conn = pool
        .get()
        .expect("Failed to get a connection from the pool");

    diesel::insert_into(accounts::table)
        .values(&new_account)
        .execute(&mut conn)
        .expect("Error inserting new account");

    HttpResponse::Created().json(new_account)
}

#[get("/accounts/{id}")]
async fn get_account(account_id: web::Path<i32>, pool: web::Data<SqlitePool>) -> impl Responder {
    use self::schema::accounts::dsl::*;

    let mut conn = pool
        .get()
        .expect("Failed to get a connection from the pool");

    let result = accounts
        .filter(id.eq(account_id.into_inner()))
        .first::<Account>(&mut conn)
        .optional()
        .expect("Error loading account");

    match result {
        Some(account) => HttpResponse::Ok().json(account),
        None => HttpResponse::NotFound().body("Account not found"),
    }
}

#[get("/accounts")]
async fn get_accounts(pool: web::Data<SqlitePool>) -> impl Responder {
    use self::schema::accounts::dsl::*;

    let mut conn = pool
        .get()
        .expect("Failed to get a connection from the pool");

    let results = accounts
        .load::<Account>(&mut conn)
        .expect("Error loading accounts");

    HttpResponse::Ok().json(results)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv().ok();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let manager = ConnectionManager::<SqliteConnection>::new(database_url.clone());

    let pool = Pool::builder()
        .max_size(10)
        .build(manager)
        .unwrap_or_else(|_| panic!("Error building connection pool"));

    let app_data = web::Data::new(pool);

    HttpServer::new(move || {
        App::new()
            .app_data(app_data.clone())
            .service(get_account)
            .service(get_accounts)
            .service(get_transaction)
            .service(get_transactions)
            .service(get_categories)
            .service(get_users)
            .service(create_user)
            .service(create_category)
            .service(create_account)
            .service(create_transaction)
    })
    .bind(("localhost", 8007))?
    .run()
    .await
}
