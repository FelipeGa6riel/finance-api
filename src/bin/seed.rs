use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use dotenvy::dotenv;
use finance_api::db::now_utc;
use finance_api::defaults::seed_user_defaults;
use finance_api::models::{Account, AccountTransfer, Category, Transaction, Type, User};
use finance_api::schema::{
    account_transfers, accounts, categories, cycle_configs, recurring_rules, tags,
    transaction_tags, transactions, transfer_tags, users,
};
use std::env;

fn main() {
    dotenv().ok();

    if env::var("ALLOW_SEED").as_deref() != Ok("1") {
        eprintln!("Recusado: defina ALLOW_SEED=1 para rodar o seed (este comando apaga dados).");
        std::process::exit(1);
    }

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let mut conn = SqliteConnection::establish(&database_url)
        .unwrap_or_else(|_| panic!("Error connecting to {database_url}"));

    let non_seed_users: i64 = users::table
        .filter(users::email.not_like("%@example.com"))
        .count()
        .get_result(&mut conn)
        .expect("Failed to check existing users");

    if non_seed_users > 0 {
        eprintln!(
            "Recusado: existem {non_seed_users} usuário(s) reais no banco. Seed só pode rodar em banco de desenvolvimento."
        );
        std::process::exit(1);
    }

    println!("Seeding database...");

    diesel::delete(transfer_tags::table).execute(&mut conn).expect("delete transfer_tags");
    diesel::delete(transaction_tags::table).execute(&mut conn).expect("delete transaction_tags");
    diesel::delete(transactions::table).execute(&mut conn).expect("delete transactions");
    diesel::delete(account_transfers::table).execute(&mut conn).expect("delete account_transfers");
    diesel::delete(recurring_rules::table).execute(&mut conn).expect("delete recurring_rules");
    diesel::delete(cycle_configs::table).execute(&mut conn).expect("delete cycle_configs");
    diesel::delete(tags::table).execute(&mut conn).expect("delete tags");
    diesel::delete(categories::table).execute(&mut conn).expect("delete categories");
    diesel::delete(accounts::table).execute(&mut conn).expect("delete accounts");
    diesel::delete(users::table).execute(&mut conn).expect("delete users");

    let current_time = now_utc();

    let new_user = User {
        id: "1".to_string(),
        name: "Felipe Silva".to_string(),
        currency: Some("BRL".to_string()),
        email: "felipe@example.com".to_string(),
        password: Some("$2a$12$t.XtIm8KrmUGd8yv88GIa.0ylr1GBDT8Rn6vJYV5qiZdrFLXb6z02".to_string()),
        cycle_start_day: 20,
        default_savings_percent: 35.0,
        deleted_at: None,
        created_at: current_time,
        updated_at: current_time,
    };

    let user_two = User {
        id: "2".to_string(),
        name: "Ana Costa".to_string(),
        currency: Some("BRL".to_string()),
        email: "ana.costa@example.com".to_string(),
        password: Some("$2a$12$t.XtIm8KrmUGd8yv88GIa.0ylr1GBDT8Rn6vJYV5qiZdrFLXb6z02".to_string()),
        cycle_start_day: 1,
        default_savings_percent: 20.0,
        deleted_at: None,
        created_at: current_time,
        updated_at: current_time,
    };

    diesel::insert_into(users::table)
        .values(&vec![new_user, user_two])
        .execute(&mut conn)
        .expect("Error inserting users");

    seed_user_defaults(&mut conn, "1").expect("seed defaults user 1");
    seed_user_defaults(&mut conn, "2").expect("seed defaults user 2");

    let accs = vec![
        Account {
            id: "1".to_string(),
            name: "Banco Principal".to_string(),
            icon: Some("🏦".to_string()),
            balance: 7849_50,
            bank_name: "Bradesco".to_string(),
            account_type: "checking".to_string(),
            user_id: "1".to_string(),
            deleted_at: None,
            created_at: current_time,
            updated_at: current_time,
        },
        Account {
            id: "2".to_string(),
            name: "Carteira".to_string(),
            icon: Some("💵".to_string()),
            balance: 2000_00,
            bank_name: "Dinheiro".to_string(),
            account_type: "cash".to_string(),
            user_id: "2".to_string(),
            deleted_at: None,
            created_at: current_time,
            updated_at: current_time,
        },
        Account {
            id: "3".to_string(),
            name: "Investment Account".to_string(),
            icon: Some("📈".to_string()),
            balance: 700_00,
            bank_name: "XP Investimentos".to_string(),
            account_type: "investment".to_string(),
            user_id: "2".to_string(),
            deleted_at: None,
            created_at: current_time,
            updated_at: current_time,
        },
    ];
    diesel::insert_into(accounts::table)
        .values(&accs)
        .execute(&mut conn)
        .expect("Error inserting accounts");

    let salary_cat_1 = categories::table
        .filter(categories::user_id.eq("1"))
        .filter(categories::is_salary.eq(true))
        .first::<Category>(&mut conn)
        .expect("salary cat");
    let food_cat_1 = categories::table
        .filter(categories::user_id.eq("1"))
        .filter(categories::name.eq("Comida"))
        .first::<Category>(&mut conn)
        .expect("food cat");

    let txs = vec![
        Transaction {
            id: "1".to_string(),
            amount: 8000_00,
            description: Some("Proventos".to_string()),
            date: current_time,
            type_: Type::Income,
            payment_method: "salary_account".to_string(),
            card_name: None,
            account_id: "1".to_string(),
            transfer_id: None,
            category_id: Some(salary_cat_1.id),
            is_fixed: Some(false),
            is_informational: false,
            credit_paid: false,
            installment_group_id: None,
            installment_number: None,
            installment_total: None,
            recurring_rule_id: None,
            needs_confirmation: false,
            user_id: "1".to_string(),
            deleted_at: None,
            created_at: current_time,
            updated_at: current_time,
        },
        Transaction {
            id: "2".to_string(),
            amount: 150_50,
            description: Some("Supermercado".to_string()),
            date: current_time,
            type_: Type::Expense,
            payment_method: "debit".to_string(),
            card_name: Some("Bradesco".to_string()),
            account_id: "1".to_string(),
            transfer_id: None,
            category_id: Some(food_cat_1.id),
            is_fixed: Some(false),
            is_informational: false,
            credit_paid: false,
            installment_group_id: None,
            installment_number: None,
            installment_total: None,
            recurring_rule_id: None,
            needs_confirmation: false,
            user_id: "1".to_string(),
            deleted_at: None,
            created_at: current_time,
            updated_at: current_time,
        },
    ];
    diesel::insert_into(transactions::table)
        .values(&txs)
        .execute(&mut conn)
        .expect("Error inserting transactions");

    let tr = AccountTransfer {
        id: "1".to_string(),
        from_account_id: "2".to_string(),
        to_account_id: Some("3".to_string()),
        amount: 500_00,
        description: Some("Aporte mensal".to_string()),
        date: current_time,
        transfer_kind: "investment".to_string(),
        user_id: "2".to_string(),
        deleted_at: None,
        created_at: current_time,
        updated_at: current_time,
    };
    diesel::insert_into(account_transfers::table)
        .values(&tr)
        .execute(&mut conn)
        .expect("Error inserting transfer");

    println!("Database seeded successfully!");
}
