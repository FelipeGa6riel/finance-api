use crate::models::{Account, Category, Transaction, User};
use crate::schema::{accounts, categories, transactions, users};
use actix_web::{HttpResponse, Responder, get, post, web};
use diesel::SqliteConnection;
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, Pool};
type SqlitePool = Pool<ConnectionManager<SqliteConnection>>;

#[post("/transactions")]
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

#[get("/transactions")]
async fn get_transactions(pool: web::Data<SqlitePool>) -> impl Responder {
    use crate::schema::transactions::dsl::*;

    let mut conn = pool
        .get()
        .expect("Failed to get a connection from the pool");

    let results = transactions
        .load::<Transaction>(&mut conn)
        .expect("Error loading transactions");

    HttpResponse::Ok().json(results)
}

#[get("/transactions/{id}")]
async fn get_transaction(tx_id: web::Path<String>, pool: web::Data<SqlitePool>) -> impl Responder {
    use crate::schema::transactions::dsl::*;

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

#[post("/transactions/{id}")]
async fn update_transaction(
    transaction_id: web::Path<String>,
    transaction: web::Json<Transaction>,
    pool: web::Data<SqlitePool>,
) -> impl Responder {
    let updated_transaction = transaction.into_inner();

    let mut conn = pool
        .get()
        .expect("Failed to get a connection from the pool");

    diesel::update(transactions::table)
        .filter(transactions::id.eq(transaction_id.into_inner()))
        .set(&updated_transaction)
        .execute(&mut conn)
        .expect("Error updating transaction");

    HttpResponse::Ok().json(updated_transaction)
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

#[post("/users/{id}")]
async fn update_user(
    user_id: web::Path<String>,
    user: web::Json<User>,
    pool: web::Data<SqlitePool>,
) -> impl Responder {
    let updated_user = user.into_inner();

    let mut conn = pool
        .get()
        .expect("Failed to get a connection from the pool");

    diesel::update(users::table)
        .filter(users::id.eq(user_id.into_inner()))
        .set(&updated_user)
        .execute(&mut conn)
        .expect("Error updating user");

    HttpResponse::Ok().json(updated_user)
}

#[get("/users/{id}")]
async fn get_user(user_id: web::Path<String>, pool: web::Data<SqlitePool>) -> impl Responder {
    use crate::schema::users::dsl::*;

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
    use crate::schema::users::dsl::*;

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
    use crate::schema::categories::dsl::*;

    let mut conn = pool
        .get()
        .expect("Failed to get a connection from the pool");

    let results = categories
        .load::<Category>(&mut conn)
        .expect("Error loading categories");

    HttpResponse::Ok().json(results)
}

#[get("/categories/{id}")]
async fn get_category(
    category_id: web::Path<String>,
    pool: web::Data<SqlitePool>,
) -> impl Responder {
    use crate::schema::categories::dsl::*;

    let mut conn = pool
        .get()
        .expect("Failed to get a connection from the pool");

    let result = categories
        .filter(id.eq(category_id.into_inner()))
        .first::<Category>(&mut conn)
        .optional()
        .expect("Error loading category");

    match result {
        Some(category) => HttpResponse::Ok().json(category),
        None => HttpResponse::NotFound().body("Category not found"),
    }
}

#[post("/categories/{id}")]
async fn update_category(
    category_id: web::Path<String>,
    category: web::Json<Category>,
    pool: web::Data<SqlitePool>,
) -> impl Responder {
    let updated_category = category.into_inner();

    let mut conn = pool
        .get()
        .expect("Failed to get a connection from the pool");

    diesel::update(categories::table)
        .filter(categories::id.eq(category_id.into_inner()))
        .set(&updated_category)
        .execute(&mut conn)
        .expect("Error updating category");

    HttpResponse::Ok().json(updated_category)
}

#[post("/accounts")]
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
async fn get_account(account_id: web::Path<String>, pool: web::Data<SqlitePool>) -> impl Responder {
    use crate::schema::accounts::dsl::*;

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
    use crate::schema::accounts::dsl::*;

    let mut conn = pool
        .get()
        .expect("Failed to get a connection from the pool");

    let results = accounts
        .load::<Account>(&mut conn)
        .expect("Error loading accounts");

    HttpResponse::Ok().json(results)
}
