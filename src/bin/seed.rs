use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use dotenvy::dotenv;
use std::env;

#[path = "../models.rs"]
mod models;
#[path = "../schema.rs"]
mod schema;

use models::{User, Category, Account, Transaction, Type, AccountTransfer};

fn main() {
    dotenv().ok();
    
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let mut conn = SqliteConnection::establish(&database_url)
        .unwrap_or_else(|_| panic!("Error connecting to {}", database_url));

    println!("Seeding database...");

    // Helper to clear existing data to start fresh
    diesel::delete(schema::account_transfers::table).execute(&mut conn).expect("Failed to delete transfers");
    diesel::delete(schema::transactions::table).execute(&mut conn).expect("Failed to delete transactions");
    diesel::delete(schema::categories::table).execute(&mut conn).expect("Failed to delete categories");
    diesel::delete(schema::accounts::table).execute(&mut conn).expect("Failed to delete accounts");
    diesel::delete(schema::users::table).execute(&mut conn).expect("Failed to delete users");

    // Current datetime for created/updated
    let now = time::OffsetDateTime::now_utc();
    let current_time = time::PrimitiveDateTime::new(now.date(), now.time());

    let new_user = User {
        id: "1".to_string(),
        name: "Felipe Silva".to_string(),
        email: "felipe@example.com".to_string(),
        password: Some("password123".to_string()),
        created_at: current_time,
        updated_at: current_time,
    };

    let user_two = User {
        id: "2".to_string(),
        name: "Ana Costa".to_string(),
        email: "ana.costa@example.com".to_string(),
        password: Some("password456".to_string()),
        created_at: current_time,
        updated_at: current_time,
    };

    diesel::insert_into(schema::users::table)
        .values(&vec![new_user.clone(), user_two.clone()])
        .execute(&mut conn)
        .expect("Error inserting users");

    let new_account = Account {
        id: "1".to_string(),
        name: "Bank".to_string(),
        balance: 8000_00,
        bank_name: "Bradesco".to_string(),
        user_id: "1".to_string(),
        created_at: current_time,
        updated_at: current_time,
    };

    let account_two = Account {
        id: "2".to_string(),
        name: "cash".to_string(),
        balance: 1000_00,
        bank_name: "".to_string(),
        user_id: "2".to_string(),
        created_at: current_time,
        updated_at: current_time,
    };

    let account_three = Account {
        id: "3".to_string(),
        name: "Investment Account".to_string(),
        balance: 200_00,
        bank_name: "XP Investimentos".to_string(),
        user_id: "2".to_string(),
        created_at: current_time,
        updated_at: current_time,
    };

    diesel::insert_into(schema::accounts::table)
        .values(&vec![new_account.clone(), account_two.clone(), account_three.clone()])
        .execute(&mut conn)
        .expect("Error inserting accounts");

    let new_category_salary = Category {
        id: "1".to_string(),
        name: "Salary".to_string(),
        description: Some("Monthly income".to_string()),
        color: Some("#4CAF50".to_string()),
        icon: Some("💼".to_string()),
        user_id: Some("1".to_string()),
        created_at: current_time,
        updated_at: current_time,
    };

    let new_category_food = Category {
        id: "2".to_string(),
        name: "Food".to_string(),
        description: Some("Groceries and dining out".to_string()),
        color: Some("#FF5722".to_string()),
        icon: Some("🍔".to_string()),
        user_id: Some("1".to_string()),
        created_at: current_time,
        updated_at: current_time,
    };
    
    let category_freelance = Category {
        id: "3".to_string(),
        name: "Freelance".to_string(),
        description: Some("Extra income".to_string()),
        color: Some("#2196F3".to_string()),
        icon: Some("💻".to_string()),
        user_id: Some("2".to_string()),
        created_at: current_time,
        updated_at: current_time,
    };

    diesel::insert_into(schema::categories::table)
        .values(&vec![new_category_salary.clone(), new_category_food.clone(), category_freelance.clone()])
        .execute(&mut conn)
        .expect("Error inserting categories");

    let new_transaction_income = Transaction {
        id: "1".to_string(),
        amount: 8000_00, // $8,000.00
        description: Some("March Salary".to_string()),
        date: current_time,
        type_: Type::Income,
        account_id: "1".to_string(),
        transfer_id: None,
        category_id: Some(new_category_salary.id.clone()),
        user_id: "1".to_string(),
        created_at: current_time,
        updated_at: current_time,
    };

    let new_transaction_expense = Transaction {
        id: "2".to_string(),
        amount: 150_50, // $150.50
        description: Some("Supermarket".to_string()),
        date: current_time,
        type_: Type::Expense,
        account_id: "1".to_string(),
        transfer_id: None,
        category_id: Some(new_category_food.id.clone()),
        user_id: "1".to_string(),
        created_at: current_time,
        updated_at: current_time,
    };

    let new_transaction_freelance = Transaction {
        id: "3".to_string(),
        amount: 2500_00,
        description: Some("Web Project".to_string()),
        date: current_time,
        type_: Type::Income,
        account_id: "2".to_string(),
        transfer_id: None,
        category_id: Some(category_freelance.id.clone()),
        user_id: "2".to_string(),
        created_at: current_time,
        updated_at: current_time,
    };

    let transfer = AccountTransfer {
        id: "1".to_string(),
        from_account_id: "2".to_string(),
        to_account_id: "3".to_string(),
        amount: 500_00,
        description: Some("Monthly Investment".to_string()),
        date: current_time,
        user_id: "2".to_string(),
        created_at: current_time,
        updated_at: current_time,
    };

    diesel::insert_into(schema::account_transfers::table)
        .values(&transfer)
        .execute(&mut conn)
        .expect("Error inserting transfer");

    let transfer_out = Transaction {
        id: "4".to_string(),
        amount: 500_00,
        description: Some("Transfer to Investment".to_string()),
        date: current_time,
        type_: Type::Expense,
        account_id: "2".to_string(),
        transfer_id: Some(transfer.id.clone()),
        category_id: None,
        user_id: "2".to_string(),
        created_at: current_time,
        updated_at: current_time,
    };

    let transfer_in = Transaction {
        id: "5".to_string(),
        amount: 500_00,
        description: Some("Transfer from Savings".to_string()),
        date: current_time,
        type_: Type::Income,
        account_id: "3".to_string(),
        transfer_id: Some(transfer.id.clone()),
        category_id: None,
        user_id: "2".to_string(),
        created_at: current_time,
        updated_at: current_time,
    };

    diesel::insert_into(schema::transactions::table)
        .values(&vec![
            new_transaction_income,
            new_transaction_expense,
            new_transaction_freelance,
            transfer_out,
            transfer_in
        ])
        .execute(&mut conn)
        .expect("Error inserting transactions");

    println!("Database seeded successfully!");
}
