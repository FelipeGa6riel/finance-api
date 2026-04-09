use std::{collections::HashMap, sync::Mutex};

use actix_web::{
    App, HttpResponse, HttpServer, Responder, get, post,
    web::{self},
};
use serde::Deserialize;
use time::{Date, PrimitiveDateTime, Time, macros::time};

time::serde::format_description!(date_format, PrimitiveDateTime, "[year]-[month]-[day] [hour]:[minute]:[second]");

#[derive(serde::Serialize, Deserialize, Clone)]
struct User {
    id: i32,
    name: String,
    email: String,
    #[serde(with = "date_format")]
    created_at: PrimitiveDateTime,
    #[serde(with = "date_format")]
    updated_at: PrimitiveDateTime,
}

#[derive(serde::Serialize, Deserialize, Clone)]
struct Category {
    id: i32,
    name: String,
    description: String,
    #[serde(with = "date_format")]
    created_at: PrimitiveDateTime,
    #[serde(with = "date_format")]
    updated_at: PrimitiveDateTime,
}

#[derive(serde::Serialize, Deserialize, Clone)]
struct Transaction {
    id: i32,
    user: User,
    amount: f64,
    category: Category,
    #[serde(with = "date_format")]
    created_at: PrimitiveDateTime,
    #[serde(with = "date_format")]
    updated_at: PrimitiveDateTime,
}

#[get("/transactions")]
async fn get_transactions(
    transactions: web::Data<Mutex<HashMap<i32, Transaction>>>,
) -> impl Responder {
    let transactions_lock = transactions.lock().unwrap();
    let vals: Vec<Transaction> = transactions_lock.values().cloned().collect();
    HttpResponse::Ok().json(vals)
}

#[get("/transactions/{id}")]
async fn get_transaction(
    id: web::Path<i32>,
    transactions: web::Data<Mutex<HashMap<i32, Transaction>>>,
) -> impl Responder {
    let transactions_lock = transactions.lock().unwrap();
    match transactions_lock.get(&id.into_inner()) {
        Some(transaction) => HttpResponse::Ok().json(transaction),
        None => HttpResponse::NotFound().finish(),
    }
}

#[post("/transaction")]
async fn create_transaction(
    transaction: web::Json<Transaction>,
    transactions: web::Data<Mutex<HashMap<i32, Transaction>>>,
) -> impl Responder {
    let mut transactions_lock = transactions.lock().unwrap();
    let new_transaction = transaction.into_inner();
    transactions_lock.insert(new_transaction.id, new_transaction.clone());
    HttpResponse::Created().json(new_transaction)
}

#[post("/user/{id}/transaction")]
async fn create_transaction_for_user(
    id: web::Path<i32>,
    transaction: web::Json<Transaction>,
    transactions: web::Data<Mutex<HashMap<i32, Transaction>>>,
) -> impl Responder {
    let mut transactions_lock = transactions.lock().unwrap();
    let new_transaction = transaction.into_inner();
    if new_transaction.user.id != id.into_inner() {
        return HttpResponse::BadRequest().body("User ID in path does not match User ID in transaction");
    }
    transactions_lock.insert(new_transaction.id, new_transaction.clone());
    HttpResponse::Created().json(new_transaction)
}

#[post("/category/{id}/transaction")]
async fn create_transaction_for_category(
    id: web::Path<i32>,
    transaction: web::Json<Transaction>,
    transactions: web::Data<Mutex<HashMap<i32, Transaction>>>,
) -> impl Responder {
    let mut transactions_lock = transactions.lock().unwrap();
    let new_transaction = transaction.into_inner();
    if new_transaction.category.id != id.into_inner() {
        return HttpResponse::BadRequest().body("Category ID in path does not match Category ID in transaction");
    }
    transactions_lock.insert(new_transaction.id, new_transaction.clone());
    HttpResponse::Created().json(new_transaction)
}

#[post("/user")]
async fn create_user(user: web::Json<User>, transactions: web::Data<Mutex<HashMap<i32, Transaction>>>) -> impl Responder {
    let mut _transactions_lock = transactions.lock().unwrap();
    let new_user = user.into_inner();
    HttpResponse::NotImplemented().finish()
}


#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let mut mem_db: HashMap<i32, Transaction> = HashMap::new();

    let trasaction = vec![
        Transaction {
            id: 1,
            user: User {
                id: 1,
                name: "John Doe".to_string(),
                email: "john.doe@example.com".to_string(),
                created_at: PrimitiveDateTime::new(Date::from_ordinal_date(2024,03).unwrap(), Time::MIDNIGHT),
                updated_at: PrimitiveDateTime::new(Date::from_ordinal_date(2024,03).unwrap(), Time::MIDNIGHT),
            },
            amount: 100.0,
            category: Category {
                id: 1,
                name: "Food".to_string(),
                description: "Expenses on food and dining".to_string(),
                created_at: PrimitiveDateTime::new(Date::from_ordinal_date(2023, 1).unwrap(), Time::MIDNIGHT),
                updated_at: PrimitiveDateTime::new(Date::from_ordinal_date(2023, 1).unwrap(), Time::MIDNIGHT),
            },
            created_at: PrimitiveDateTime::new(Date::from_ordinal_date(2023, 1).unwrap(), Time::MIDNIGHT),
            updated_at: PrimitiveDateTime::new(Date::from_ordinal_date(2023, 1).unwrap(), Time::MIDNIGHT),
        },
        Transaction {
            id: 2,
            user: User {
                id: 2,
                name: "Jane Smith".to_string(),
                email: "jane.smith@example.com".to_string(),
                created_at: PrimitiveDateTime::new(Date::from_ordinal_date(2023, 1).unwrap(), time!(12:00:00)),
                updated_at: PrimitiveDateTime::new(Date::from_ordinal_date(2023, 1).unwrap(), time!(12:00:00)),
            },
            amount: 50.0,
            category: Category {
                id: 2,
                name: "Transport".to_string(),
                description: "Expenses on transportation".to_string(),
                created_at: PrimitiveDateTime::new(Date::from_ordinal_date(2023, 1).unwrap(), time!(15:00:00)),
                updated_at: PrimitiveDateTime::new(Date::from_ordinal_date(2023, 1).unwrap(), time!(15:00:00)),
            },
            created_at: PrimitiveDateTime::new(Date::from_ordinal_date(2023, 1).unwrap(), time!(15:00:00)),
            updated_at: PrimitiveDateTime::new(Date::from_ordinal_date(2023, 1).unwrap(), time!(15:00:00)),
        },
    ];

    for transaction in trasaction {
        mem_db.insert(transaction.id, transaction);
    }

    let app_data = web::Data::new(Mutex::new(mem_db));

    HttpServer::new(move || {
        App::new()
            .app_data(app_data.clone())
            .service(get_transactions)
            .service(create_transaction)
            .service(get_transaction)
    })
    .bind(("localhost", 8007))?
    .run()
    .await
}
