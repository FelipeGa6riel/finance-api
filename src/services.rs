use crate::models::{Account, Category, Transaction, User};
use crate::schema::{accounts, categories, transactions, users};
use actix_web::{HttpResponse, Responder, get, post, web};
use diesel::SqliteConnection;
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, Pool};
use jsonwebtoken::{EncodingKey, Header, encode};
use std::time::SystemTime;
type SqlitePool = Pool<ConnectionManager<SqliteConnection>>;
use serde::{Deserialize, Serialize};

#[allow(dead_code)]
#[derive(Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: String,
    pub exp: usize,
}

#[allow(dead_code)]
#[derive(Serialize, Deserialize)]
struct LoginRequest {
    email: String,
    password: String,
}

#[post("/login")]
async fn login(
    credentials: web::Json<LoginRequest>,
    pool: web::Data<SqlitePool>,
) -> impl Responder {
    use crate::schema::users::dsl::*;

    let mut conn = match pool.get() {
        Ok(connection) => connection,
        Err(_) => {
            return HttpResponse::InternalServerError()
                .body("Failed to get a connection from the pool");
        }
    };

    let user = users
        .filter(email.eq(&credentials.email))
        .first::<User>(&mut conn)
        .optional()
        .expect("Error loading user");

    match user {
        Some(user) => {
            let hash_password = user.password.unwrap_or_default();

            let is_valid = bcrypt::verify(&credentials.password, &hash_password).unwrap_or(false);

            if is_valid {
                let expiration = SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .expect("Time went backwards")
                    .as_secs()
                    + 24 * 3600; // 24h

                let claims = Claims {
                    sub: user.id.clone(),
                    exp: expiration as usize,
                };

                let secret_token =
                    std::env::var("SECRET_KEY").unwrap_or_else(|_| "default_secret".to_string());

                let token = encode(
                    &Header::default(),
                    &claims,
                    &EncodingKey::from_secret(secret_token.as_ref()),
                )
                .unwrap();

                HttpResponse::Ok().json(serde_json::json!({ "token": token }))
            } else {
                HttpResponse::Unauthorized().body("Invalid credentials")
            }
        }
        None => HttpResponse::Unauthorized().body("Invalid credentials"),
    }
}

#[post("/transactions")]
async fn create_transaction(
    transaction: web::Json<Transaction>,
    pool: web::Data<SqlitePool>,
    claims: web::ReqData<Claims>,
) -> impl Responder {
    let new_transaction = transaction.into_inner();

    let mut conn = pool
        .get()
        .expect("Failed to get a connection from the pool");

    let hash_user_id = &claims.sub;

    let transaction_with_user = Transaction {
        user_id: hash_user_id.clone(),
        ..new_transaction
    };

    diesel::insert_into(transactions::table)
        .values(&transaction_with_user)
        .execute(&mut conn)
        .expect("Error inserting new transaction");

    HttpResponse::Created().json(transaction_with_user)
}

#[get("/transactions")]
async fn get_transactions(
    pool: web::Data<SqlitePool>,
    claims: web::ReqData<Claims>,
) -> impl Responder {
    use crate::schema::transactions::dsl::*;

    let mut conn = pool
        .get()
        .expect("Failed to get a connection from the pool");

    let hash_user_id = &claims.sub;

    let results = transactions
        .filter(user_id.eq(hash_user_id))
        .load::<Transaction>(&mut conn)
        .expect("Error loading transactions");

    HttpResponse::Ok().json(results)
}

// #[get("/transactions/user/{id_user}")]
// async fn get_transactions_by_user(
//     id_user: web::Path<String>,
//     pool: web::Data<SqlitePool>,
// ) -> impl Responder {
//     use crate::schema::transactions::dsl::*;

//     let mut conn = pool
//         .get()
//         .expect("Failed to get a connection from the pool");

//     let results = transactions
//         .filter(user_id.eq(id_user.into_inner()))
//         .load::<Transaction>(&mut conn)
//         .expect("Error loading transactions");

//     HttpResponse::Ok().json(results)
// }

#[get("/transactions/{id}")]
async fn get_transaction(
    tx_id: web::Path<String>,
    pool: web::Data<SqlitePool>,
    claims: web::ReqData<Claims>,
) -> impl Responder {
    use crate::schema::transactions::dsl::*;

    let mut conn = pool
        .get()
        .expect("Failed to get a connection from the pool");

    let hash_user_id = &claims.sub;

    let result = transactions
        .filter(id.eq(tx_id.into_inner()).and(user_id.eq(hash_user_id)))
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
    claims: web::ReqData<Claims>,
) -> impl Responder {
    let updated_transaction = transaction.into_inner();

    let mut conn = pool
        .get()
        .expect("Failed to get a connection from the pool");

    let hash_user_id = &claims.sub;

    diesel::update(transactions::table)
        .filter(
            transactions::id
                .eq(transaction_id.into_inner())
                .and(transactions::user_id.eq(hash_user_id)),
        )
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

    let hashed_password = match &new_user.password {
        Some(pw) => bcrypt::hash(pw, bcrypt::DEFAULT_COST).unwrap_or_default(),
        None => return HttpResponse::BadRequest().body("Password is required"),
    };

    let user_with_hashed_password = User {
        password: Some(hashed_password),
        ..new_user
    };

    diesel::insert_into(users::table)
        .values(&user_with_hashed_password)
        .execute(&mut conn)
        .expect("Error inserting new user");

    HttpResponse::Created().json(user_with_hashed_password)
}

#[post("/users/{id}")]
async fn update_user(
    user_id: web::Path<String>,
    user: web::Json<User>,
    pool: web::Data<SqlitePool>,
    claims: web::ReqData<Claims>,
) -> impl Responder {
    let updated_user = user.into_inner();

    let mut conn = pool
        .get()
        .expect("Failed to get a connection from the pool");

    let hash_user_id = &claims.sub;

    diesel::update(users::table)
        .filter(
            users::id
                .eq(user_id.into_inner())
                .and(users::id.eq(hash_user_id)),
        )
        .set(&updated_user)
        .execute(&mut conn)
        .expect("Error updating user");

    HttpResponse::Ok().json(updated_user)
}

#[get("/users/{id}")]
async fn get_user(
    user_id: web::Path<String>,
    pool: web::Data<SqlitePool>,
    claims: web::ReqData<Claims>,
) -> impl Responder {
    use crate::schema::users::dsl::*;

    let mut conn = pool
        .get()
        .expect("Failed to get a connection from the pool");

    let hash_user_id = &claims.sub;

    let result = users
        .filter(id.eq(user_id.into_inner()).and(id.eq(hash_user_id)))
        .first::<User>(&mut conn)
        .optional()
        .expect("Error loading user");

    match result {
        Some(user) => HttpResponse::Ok().json(user),
        None => HttpResponse::NotFound().body("User not found"),
    }
}

#[get("/users")]
async fn get_users(pool: web::Data<SqlitePool>, claims: web::ReqData<Claims>) -> impl Responder {
    use crate::schema::users::dsl::*;

    let mut conn = pool
        .get()
        .expect("Failed to get a connection from the pool");

    let hash_user_id = &claims.sub;

    let results = users
        .filter(id.eq(hash_user_id))
        .load::<User>(&mut conn)
        .expect("Error loading users");

    HttpResponse::Ok().json(results)
}

#[post("/categories")]
async fn create_category(
    category: web::Json<Category>,
    pool: web::Data<SqlitePool>,
    claims: web::ReqData<Claims>,
) -> impl Responder {
    let new_category = category.into_inner();

    let mut conn = pool
        .get()
        .expect("Failed to get a connection from the pool");

    let hash_user_id = &claims.sub;
    let category_with_user = Category {
        user_id:Some(hash_user_id.clone()),
        ..new_category
    };
    diesel::insert_into(categories::table)
        .values(&category_with_user)
        .execute(&mut conn)
        .expect("Error inserting new category");

    HttpResponse::Created().json(category_with_user)
}

#[get("/categories")]
async fn get_categories(pool: web::Data<SqlitePool>, claims: web::ReqData<Claims>) -> impl Responder {
    use crate::schema::categories::dsl::*;

    let mut conn = pool
        .get()
        .expect("Failed to get a connection from the pool");

    let hash_user_id = &claims.sub;

    let results = categories
        .filter(user_id.eq(hash_user_id))
        .load::<Category>(&mut conn)
        .expect("Error loading categories");

    HttpResponse::Ok().json(results)
}

#[get("/categories/{id}")]
async fn get_category(
    category_id: web::Path<String>,
    pool: web::Data<SqlitePool>,
    claims: web::ReqData<Claims>,
) -> impl Responder {
    use crate::schema::categories::dsl::*;

    let mut conn = pool
        .get()
        .expect("Failed to get a connection from the pool");

    let hash_user_id = &claims.sub;

    let result = categories
        .filter(id.eq(category_id.into_inner()).and(user_id.eq(hash_user_id)))
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
    claims: web::ReqData<Claims>,
) -> impl Responder {
    let updated_category = category.into_inner();

    let mut conn = pool
        .get()
        .expect("Failed to get a connection from the pool");

    let hash_user_id = &claims.sub;

    diesel::update(categories::table)
        .filter(categories::id.eq(category_id.into_inner()).and(categories::user_id.eq(hash_user_id)))
        .set(&updated_category)
        .execute(&mut conn)
        .expect("Error updating category");

    HttpResponse::Ok().json(updated_category)
}

#[post("/accounts")]
async fn create_account(
    account: web::Json<Account>,
    pool: web::Data<SqlitePool>,
    claims: web::ReqData<Claims>,
) -> impl Responder {
    let new_account = account.into_inner();

    let mut conn = pool
        .get()
        .expect("Failed to get a connection from the pool");

    let hash_user_id = &claims.sub;
    let account_with_user = Account {
        user_id: hash_user_id.clone(),
        ..new_account
    };

    diesel::insert_into(accounts::table)
        .values(&account_with_user)
        .execute(&mut conn)
        .expect("Error inserting new account");

    HttpResponse::Created().json(account_with_user)
}

#[get("/accounts/{id}")]
async fn get_account(account_id: web::Path<String>, pool: web::Data<SqlitePool>, claims: web::ReqData<Claims>) -> impl Responder {
    use crate::schema::accounts::dsl::*;

    let mut conn = pool
        .get()
        .expect("Failed to get a connection from the pool");

    let hash_user_id = &claims.sub;

    let result = accounts
        .filter(id.eq(account_id.into_inner()).and(user_id.eq(hash_user_id)))
        .first::<Account>(&mut conn)
        .optional()
        .expect("Error loading account");

    match result {
        Some(account) => HttpResponse::Ok().json(account),
        None => HttpResponse::NotFound().body("Account not found"),
    }
}

#[get("/accounts")]
async fn get_accounts(pool: web::Data<SqlitePool>, claims: web::ReqData<Claims>) -> impl Responder {
    use crate::schema::accounts::dsl::*;

    let mut conn = pool
        .get()
        .expect("Failed to get a connection from the pool");

    let hash_user_id = &claims.sub;

    let results = accounts
        .filter(user_id.eq(hash_user_id))
        .load::<Account>(&mut conn)
        .expect("Error loading accounts");

    HttpResponse::Ok().json(results)
}
