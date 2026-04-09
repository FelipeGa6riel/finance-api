use std::{collections::HashMap, sync::Mutex};

use actix_web::{
    App, HttpResponse, HttpServer, Responder, get, post,
    web::{self},
};
use serde::Deserialize;

#[derive(serde::Serialize, Deserialize, Clone)]
struct Category {
    id: i32,
    name: String,
    description: String,
}

#[derive(serde::Serialize, Deserialize, Clone)]
struct Transaction {
    id: i32,
    amount: f64,
    category: Category,
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

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let mut transactions: HashMap<i32, Transaction> = HashMap::new();

    let trasaction = vec![
        Transaction {
            id: 1,
            amount: 100.0,
            category: Category {
                id: 1,
                name: "Food".to_string(),
                description: "Expenses on food and dining".to_string(),
            },
        },
        Transaction {
            id: 2,
            amount: 50.0,
            category: Category {
                id: 2,
                name: "Transport".to_string(),
                description: "Expenses on transportation".to_string(),
            },
        },
    ];

    for transaction in trasaction {
        transactions.insert(transaction.id, transaction);
    }

    let app_data = web::Data::new(Mutex::new(transactions));

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
