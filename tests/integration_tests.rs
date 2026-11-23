use actix_web::{App, http::StatusCode, test, web};
use diesel::connection::SimpleConnection;
use diesel::prelude::*;
use finance_api::auth::{AuthConfig, LoginRateLimiter, encode_token};
use finance_api::configure_app;
use finance_api::db::{SqlitePool, build_pool, now_utc};
use finance_api::defaults::seed_user_defaults;
use finance_api::models::{Category, Transaction, User};
use finance_api::schema::{categories, transactions, users};
use serde_json::{Value, json};
use std::path::PathBuf;
use uuid::Uuid;

struct TestApp {
    pool: SqlitePool,
    secret_key: String,
    db_path: PathBuf,
}

impl TestApp {
    fn new() -> Self {
        let db_path = std::env::temp_dir().join(format!("fin_test_{}.db", Uuid::new_v4()));
        let db_str = db_path.to_string_lossy().to_string();
        let pool = build_pool(&db_str);
        {
            let mut conn = pool.get().expect("conn");
            conn.batch_execute(include_str!(
                "../migrations/2026-04-18-005936-0000_setup_tables/up.sql"
            ))
            .expect("init migration");
            conn.batch_execute(include_str!(
                "../migrations/2026-11-20-120000_full_finance_schema/up.sql"
            ))
            .expect("full migration");
        }
        Self {
            pool,
            secret_key: "test-secret-key-very-strong".to_string(),
            db_path,
        }
    }

    fn create_user(&self, name: &str, email: &str, password: &str) -> (User, String) {
        let mut conn = self.pool.get().expect("conn");
        let now = now_utc();
        let hashed = bcrypt::hash(password, 4).expect("hash");
        let user = User {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            currency: Some("BRL".to_string()),
            email: email.to_lowercase(),
            password: Some(hashed),
            cycle_start_day: 20,
            default_savings_percent: 35.0,
            deleted_at: None,
            created_at: now,
            updated_at: now,
        };
        diesel::insert_into(users::table)
            .values(&user)
            .execute(&mut conn)
            .expect("insert user");
        seed_user_defaults(&mut conn, &user.id).expect("seed defaults");
        let token = encode_token(&user.id, &self.secret_key).expect("token");
        (user, token)
    }
}

impl Drop for TestApp {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.db_path);
        let _ = std::fs::remove_file(format!("{}-wal", self.db_path.display()));
        let _ = std::fs::remove_file(format!("{}-shm", self.db_path.display()));
    }
}

macro_rules! init_test_svc {
    ($app:expr, $limiter:expr) => {
        test::init_service(
            App::new()
                .app_data(web::Data::new($app.pool.clone()))
                .app_data(web::Data::new(AuthConfig {
                    secret_key: $app.secret_key.clone(),
                }))
                .app_data(web::Data::new($limiter))
                .configure(configure_app),
        )
        .await
    };
}

#[actix_web::test]
async fn test_public_signup_removed_and_forged_or_deleted_user_rejected() {
    let app = TestApp::new();
    let (user_a, token_a) = app.create_user("User A", "a@test.local", "secret123");
    let srv = init_test_svc!(app, LoginRateLimiter::new());

    // 1. Public POST /users is closed (404)
    let req = test::TestRequest::post()
        .uri("/users")
        .set_json(json!({ "name": "Hacker", "email": "h@test.local", "password": "123" }))
        .to_request();
    let resp = test::call_service(&srv, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    // 2. Forged token with legacy "default_secret" is rejected (401)
    let forged = encode_token(&user_a.id, "default_secret").expect("forged");
    let req = test::TestRequest::get()
        .uri("/api/me")
        .insert_header(("Authorization", format!("Bearer {forged}")))
        .to_request();
    let resp = test::call_service(&srv, req).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // 3. Valid token works
    let req = test::TestRequest::get()
        .uri("/api/me")
        .insert_header(("Authorization", format!("Bearer {token_a}")))
        .to_request();
    let resp = test::call_service(&srv, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    // 4. Soft-delete user -> existing JWT and /login immediately fail (401)
    {
        let mut conn = app.pool.get().unwrap();
        diesel::update(users::table.filter(users::id.eq(&user_a.id)))
            .set(users::deleted_at.eq(Some(now_utc())))
            .execute(&mut conn)
            .unwrap();
    }
    let req = test::TestRequest::get()
        .uri("/api/me")
        .insert_header(("Authorization", format!("Bearer {token_a}")))
        .to_request();
    let resp = test::call_service(&srv, req).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    let req = test::TestRequest::post()
        .uri("/login")
        .set_json(json!({ "email": "a@test.local", "password": "secret123" }))
        .to_request();
    let resp = test::call_service(&srv, req).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn test_login_rate_limiter_and_password_change() {
    let app = TestApp::new();
    let (_user_a, token_a) = app.create_user("User A", "a@test.local", "secret123");
    let srv = init_test_svc!(app, LoginRateLimiter::with_limit(2, 300));

    // Change password safely without touching /api/me password overwrite
    let req = test::TestRequest::put()
        .uri("/api/me")
        .insert_header(("Authorization", format!("Bearer {token_a}")))
        .set_json(json!({ "name": "Alice", "password": "plaintext-ignored" }))
        .to_request();
    let resp = test::call_service(&srv, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    // Old password still works
    let req = test::TestRequest::post()
        .uri("/login")
        .set_json(json!({ "email": "a@test.local", "password": "secret123" }))
        .to_request();
    let resp = test::call_service(&srv, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    // Change password endpoint
    let req = test::TestRequest::post()
        .uri("/api/me/password")
        .insert_header(("Authorization", format!("Bearer {token_a}")))
        .set_json(json!({ "current_password": "secret123", "new_password": "newsecret456" }))
        .to_request();
    let resp = test::call_service(&srv, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    // 2nd attempt hits limit=2 -> succeeds; 3rd attempt -> 429 Too Many Requests
    let req = test::TestRequest::post()
        .uri("/login")
        .set_json(json!({ "email": "a@test.local", "password": "newsecret456" }))
        .to_request();
    assert_eq!(test::call_service(&srv, req).await.status(), StatusCode::OK);

    let req = test::TestRequest::post()
        .uri("/login")
        .set_json(json!({ "email": "a@test.local", "password": "newsecret456" }))
        .to_request();
    assert_eq!(
        test::call_service(&srv, req).await.status(),
        StatusCode::TOO_MANY_REQUESTS
    );
}

#[actix_web::test]
async fn test_multi_user_isolation_and_idor_prevention() {
    let app = TestApp::new();
    let (user_a, token_a) = app.create_user("User A", "a@test.local", "secret123");
    let (user_b, token_b) = app.create_user("User B", "b@test.local", "secret123");
    let srv = init_test_svc!(app, LoginRateLimiter::new());

    // Create account for A and B
    let req = test::TestRequest::post()
        .uri("/api/accounts")
        .insert_header(("Authorization", format!("Bearer {token_a}")))
        .set_json(json!({ "name": "Conta A", "bank_name": "Nubank", "balance": 1000_00 }))
        .to_request();
    let acc_a: Value = test::call_and_read_body_json(&srv, req).await;

    let req = test::TestRequest::post()
        .uri("/api/accounts")
        .insert_header(("Authorization", format!("Bearer {token_b}")))
        .set_json(json!({ "name": "Conta B", "bank_name": "BB", "balance": 5000_00 }))
        .to_request();
    let acc_b: Value = test::call_and_read_body_json(&srv, req).await;

    // C2 IDOR write: User A tries to create expense on User B's account -> 404, B balance untouched
    let req = test::TestRequest::post()
        .uri("/api/transactions")
        .insert_header(("Authorization", format!("Bearer {token_a}")))
        .set_json(json!({
            "amount": 999_00,
            "description": "Roubo",
            "type": "Expense",
            "payment_method": "pix",
            "account_id": acc_b["id"].as_str().unwrap(),
        }))
        .to_request();
    let resp = test::call_service(&srv, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    // Verify B balance unchanged
    let req = test::TestRequest::get()
        .uri(&format!("/api/accounts/{}", acc_b["id"].as_str().unwrap()))
        .insert_header(("Authorization", format!("Bearer {token_b}")))
        .to_request();
    let acc_b_after: Value = test::call_and_read_body_json(&srv, req).await;
    assert_eq!(acc_b_after["balance"].as_i64().unwrap(), 5000_00);

    // User A creates valid transaction, gets serialized `id` (P0-4), updates and deletes with balance reversal (A1, D8)
    let cat_b: Category = {
        let mut conn = app.pool.get().unwrap();
        categories::table
            .filter(categories::user_id.eq(&user_b.id))
            .first(&mut conn)
            .unwrap()
    };

    // M4: User A cannot link User B's category
    let req = test::TestRequest::post()
        .uri("/api/transactions")
        .insert_header(("Authorization", format!("Bearer {token_a}")))
        .set_json(json!({
            "amount": 100_00,
            "description": "Teste",
            "type": "Expense",
            "payment_method": "pix",
            "account_id": acc_a["id"].as_str().unwrap(),
            "category_id": cat_b.id,
        }))
        .to_request();
    assert_eq!(test::call_service(&srv, req).await.status(), StatusCode::NOT_FOUND);

    // Valid transaction for User A
    let req = test::TestRequest::post()
        .uri("/api/transactions")
        .insert_header(("Authorization", format!("Bearer {token_a}")))
        .set_json(json!({
            "amount": 200_00,
            "description": "Mercado",
            "type": "Expense",
            "payment_method": "debit",
            "account_id": acc_a["id"].as_str().unwrap(),
        }))
        .to_request();
    let tx_val: Value = test::call_and_read_body_json(&srv, req).await;
    let tx_id = tx_val["id"].as_str().expect("id must be serialized").to_string();
    assert_eq!(tx_val["user_id"].as_str(), Some(user_a.id.as_str()));

    // Check balance 1000 - 200 = 800
    let req = test::TestRequest::get()
        .uri(&format!("/api/accounts/{}", acc_a["id"].as_str().unwrap()))
        .insert_header(("Authorization", format!("Bearer {token_a}")))
        .to_request();
    let acc_a_1: Value = test::call_and_read_body_json(&srv, req).await;
    assert_eq!(acc_a_1["balance"].as_i64().unwrap(), 800_00);

    // User B tries to GET / PUT / DELETE User A's transaction -> all 404
    let req = test::TestRequest::get()
        .uri(&format!("/api/transactions/{tx_id}"))
        .insert_header(("Authorization", format!("Bearer {token_b}")))
        .to_request();
    assert_eq!(test::call_service(&srv, req).await.status(), StatusCode::NOT_FOUND);

    let req = test::TestRequest::delete()
        .uri(&format!("/api/transactions/{tx_id}"))
        .insert_header(("Authorization", format!("Bearer {token_b}")))
        .to_request();
    assert_eq!(test::call_service(&srv, req).await.status(), StatusCode::NOT_FOUND);

    // User A updates amount to 350 -> balance becomes 1000 - 350 = 650, user_id preserved!
    let req = test::TestRequest::put()
        .uri(&format!("/api/transactions/{tx_id}"))
        .insert_header(("Authorization", format!("Bearer {token_a}")))
        .set_json(json!({ "amount": 350_00 }))
        .to_request();
    let updated_tx: Value = test::call_and_read_body_json(&srv, req).await;
    assert_eq!(updated_tx["user_id"].as_str(), Some(user_a.id.as_str()));
    assert_eq!(updated_tx["amount"].as_i64(), Some(350_00));

    let req = test::TestRequest::get()
        .uri(&format!("/api/accounts/{}", acc_a["id"].as_str().unwrap()))
        .insert_header(("Authorization", format!("Bearer {token_a}")))
        .to_request();
    let acc_a_2: Value = test::call_and_read_body_json(&srv, req).await;
    assert_eq!(acc_a_2["balance"].as_i64().unwrap(), 650_00);

    // User A deletes transaction -> soft-deleted and balance restored to 1000
    let req = test::TestRequest::delete()
        .uri(&format!("/api/transactions/{tx_id}"))
        .insert_header(("Authorization", format!("Bearer {token_a}")))
        .to_request();
    assert_eq!(test::call_service(&srv, req).await.status(), StatusCode::NO_CONTENT);

    let req = test::TestRequest::get()
        .uri(&format!("/api/accounts/{}", acc_a["id"].as_str().unwrap()))
        .insert_header(("Authorization", format!("Bearer {token_a}")))
        .to_request();
    let acc_a_3: Value = test::call_and_read_body_json(&srv, req).await;
    assert_eq!(acc_a_3["balance"].as_i64().unwrap(), 1000_00);
}

#[actix_web::test]
async fn test_cycle_summary_investments_credit_option3_and_recurrences() {
    let app = TestApp::new();
    let (user_a, token_a) = app.create_user("User A", "a@test.local", "secret123");
    let srv = init_test_svc!(app, LoginRateLimiter::new());

    // Create checking and investment accounts
    let req = test::TestRequest::post()
        .uri("/api/accounts")
        .insert_header(("Authorization", format!("Bearer {token_a}")))
        .set_json(json!({ "name": "Banco do Brasil", "bank_name": "BB", "account_type": "checking", "balance": 0 }))
        .to_request();
    let checking: Value = test::call_and_read_body_json(&srv, req).await;

    let req = test::TestRequest::post()
        .uri("/api/accounts")
        .insert_header(("Authorization", format!("Bearer {token_a}")))
        .set_json(json!({ "name": "Corretora", "bank_name": "XP", "account_type": "investment", "balance": 0 }))
        .to_request();
    let inv_acc: Value = test::call_and_read_body_json(&srv, req).await;

    let (salary_cat, food_cat, fixed_cat, rf_tag_id) = {
        let mut conn = app.pool.get().unwrap();
        let sc: Category = categories::table
            .filter(categories::user_id.eq(&user_a.id))
            .filter(categories::is_salary.eq(true))
            .first(&mut conn)
            .unwrap();
        let fc: Category = categories::table
            .filter(categories::user_id.eq(&user_a.id))
            .filter(categories::name.eq("Comida"))
            .first(&mut conn)
            .unwrap();
        let fxc: Category = categories::table
            .filter(categories::user_id.eq(&user_a.id))
            .filter(categories::name.eq("Despesa fixa"))
            .first(&mut conn)
            .unwrap();
        let rft: String = finance_api::schema::tags::table
            .filter(finance_api::schema::tags::user_id.eq(&user_a.id))
            .filter(finance_api::schema::tags::name.eq("renda_fixa"))
            .select(finance_api::schema::tags::id)
            .first(&mut conn)
            .unwrap();
        (sc, fc, fxc, rft)
    };

    // Cycle 2026-11 (cycle_start_day=20 -> 2026-10-20 to 2026-11-19)
    // 1. Salary 5000.00
    let req = test::TestRequest::post()
        .uri("/api/transactions")
        .insert_header(("Authorization", format!("Bearer {token_a}")))
        .set_json(json!({
            "amount": 5000_00,
            "description": "Proventos",
            "date": "2026-10-20 10:00:00",
            "type": "Income",
            "payment_method": "salary_account",
            "account_id": checking["id"].as_str().unwrap(),
            "category_id": salary_cat.id,
        }))
        .to_request();
    assert_eq!(test::call_service(&srv, req).await.status(), StatusCode::CREATED);

    // 2. Fixed expense 100.00 + Variable food 200.00
    let req = test::TestRequest::post()
        .uri("/api/transactions")
        .insert_header(("Authorization", format!("Bearer {token_a}")))
        .set_json(json!({
            "amount": 100_00,
            "description": "Fibra",
            "date": "2026-10-25 10:00:00",
            "type": "Expense",
            "payment_method": "pix",
            "account_id": checking["id"].as_str().unwrap(),
            "category_id": fixed_cat.id,
        }))
        .to_request();
    assert_eq!(test::call_service(&srv, req).await.status(), StatusCode::CREATED);

    let req = test::TestRequest::post()
        .uri("/api/transactions")
        .insert_header(("Authorization", format!("Bearer {token_a}")))
        .set_json(json!({
            "amount": 200_00,
            "description": "Mercado",
            "date": "2026-10-26 10:00:00",
            "type": "Expense",
            "payment_method": "debit",
            "account_id": checking["id"].as_str().unwrap(),
            "category_id": food_cat.id,
        }))
        .to_request();
    assert_eq!(test::call_service(&srv, req).await.status(), StatusCode::CREATED);

    // 3. Investment transfer 500.00 (Q8: NOT an expense)
    let req = test::TestRequest::post()
        .uri("/api/transfers")
        .insert_header(("Authorization", format!("Bearer {token_a}")))
        .set_json(json!({
            "from_account_id": checking["id"].as_str().unwrap(),
            "to_account_id": inv_acc["id"].as_str().unwrap(),
            "amount": 500_00,
            "description": "Tesouro Direto",
            "date": "2026-10-27 10:00:00",
            "transfer_kind": "investment",
            "tag_ids": [rf_tag_id],
        }))
        .to_request();
    assert_eq!(test::call_service(&srv, req).await.status(), StatusCode::CREATED);

    // 4. Credit purchase 300.00 in 3x (100.00 per cycle: 2026-11, 2026-12, 2027-01) -> does NOT deduct checking balance now!
    let req = test::TestRequest::post()
        .uri("/api/transactions")
        .insert_header(("Authorization", format!("Bearer {token_a}")))
        .set_json(json!({
            "amount": 300_00,
            "description": "Compra Parcelada",
            "date": "2026-11-05 10:00:00",
            "type": "Expense",
            "payment_method": "credit",
            "card_name": "Nubank",
            "account_id": checking["id"].as_str().unwrap(),
            "category_id": food_cat.id,
            "installment_total": 3,
        }))
        .to_request();
    assert_eq!(test::call_service(&srv, req).await.status(), StatusCode::CREATED);

    // Check balances right now: checking = 5000 - 100 - 200 - 500 = 4200 (credit 300 NOT deducted yet!), inv_acc = 500
    let req = test::TestRequest::get()
        .uri(&format!("/api/accounts/{}", checking["id"].as_str().unwrap()))
        .insert_header(("Authorization", format!("Bearer {token_a}")))
        .to_request();
    let chk_now: Value = test::call_and_read_body_json(&srv, req).await;
    assert_eq!(chk_now["balance"].as_i64().unwrap(), 4200_00);

    // Set cycle 2026-11 savings override fixed = 2000.00
    let req = test::TestRequest::put()
        .uri("/api/cycles/2026-11/config")
        .insert_header(("Authorization", format!("Bearer {token_a}")))
        .set_json(json!({ "savings_mode": "fixed", "savings_fixed_amount": 2000_00 }))
        .to_request();
    assert_eq!(test::call_service(&srv, req).await.status(), StatusCode::OK);

    // Summary 2026-11:
    // total_income=5000, saved=2000, available=3000, expenses=100+200+100(credit parcel 1/3)=400, free_remainder=2600
    // investments_renda_fixa=500, open_credit_invoices=[Nubank: 100]
    let req = test::TestRequest::get()
        .uri("/api/cycles/summary?cycle=2026-11")
        .insert_header(("Authorization", format!("Bearer {token_a}")))
        .to_request();
    let sum: Value = test::call_and_read_body_json(&srv, req).await;
    assert_eq!(sum["total_income"].as_i64(), Some(5000_00));
    assert_eq!(sum["saved_amount"].as_i64(), Some(2000_00));
    assert_eq!(sum["available_to_spend"].as_i64(), Some(3000_00));
    assert_eq!(sum["total_expenses"].as_i64(), Some(400_00));
    assert_eq!(sum["free_remainder"].as_i64(), Some(2600_00));
    assert_eq!(sum["investments_renda_fixa"].as_i64(), Some(500_00));
    assert_eq!(sum["open_credit_invoices"][0]["total"].as_i64(), Some(100_00));

    // Pay Nubank invoice for 2026-11 (100.00) -> deducts checking to 4100.00 and closes open invoice without duplicating expenses!
    let inv_tx_ids = sum["open_credit_invoices"][0]["transaction_ids"].clone();
    let req = test::TestRequest::post()
        .uri("/api/credit/pay-invoice")
        .insert_header(("Authorization", format!("Bearer {token_a}")))
        .set_json(json!({
            "card_name": "Nubank",
            "from_account_id": checking["id"].as_str().unwrap(),
            "amount": 100_00,
            "date": "2026-11-10 10:00:00",
            "transaction_ids": inv_tx_ids,
        }))
        .to_request();
    assert_eq!(test::call_service(&srv, req).await.status(), StatusCode::CREATED);

    let req = test::TestRequest::get()
        .uri(&format!("/api/accounts/{}", checking["id"].as_str().unwrap()))
        .insert_header(("Authorization", format!("Bearer {token_a}")))
        .to_request();
    let chk_paid: Value = test::call_and_read_body_json(&srv, req).await;
    assert_eq!(chk_paid["balance"].as_i64().unwrap(), 4100_00);

    let req = test::TestRequest::get()
        .uri("/api/cycles/summary?cycle=2026-11")
        .insert_header(("Authorization", format!("Bearer {token_a}")))
        .to_request();
    let sum_after: Value = test::call_and_read_body_json(&srv, req).await;
    assert_eq!(sum_after["total_expenses"].as_i64(), Some(400_00));
    assert_eq!(sum_after["open_credit_invoices"].as_array().unwrap().len(), 0);

    // 5. Variable recurrence rule (Q10) + idempotent generation
    let req = test::TestRequest::post()
        .uri("/api/recurrences")
        .insert_header(("Authorization", format!("Bearer {token_a}")))
        .set_json(json!({
            "description": "Equatorial Energia",
            "default_amount": 220_00,
            "payment_method": "pix",
            "account_id": checking["id"].as_str().unwrap(),
            "category_id": fixed_cat.id,
            "day_of_month": 15,
            "is_variable_amount": true,
            "is_fixed": true,
        }))
        .to_request();
    assert_eq!(test::call_service(&srv, req).await.status(), StatusCode::CREATED);

    // Generate for 2026-11 twice -> 1 created first time, 0 second time!
    let req = test::TestRequest::post()
        .uri("/api/recurrences/generate")
        .insert_header(("Authorization", format!("Bearer {token_a}")))
        .set_json(json!({ "cycle_key": "2026-11" }))
        .to_request();
    let g1: Value = test::call_and_read_body_json(&srv, req).await;
    assert_eq!(g1["created_count"].as_u64(), Some(1));

    let req = test::TestRequest::post()
        .uri("/api/recurrences/generate")
        .insert_header(("Authorization", format!("Bearer {token_a}")))
        .set_json(json!({ "cycle_key": "2026-11" }))
        .to_request();
    let g2: Value = test::call_and_read_body_json(&srv, req).await;
    assert_eq!(g2["created_count"].as_u64(), Some(0));

    let mut conn = app.pool.get().unwrap();
    let gen_tx: Transaction = transactions::table
        .filter(transactions::user_id.eq(&user_a.id))
        .filter(transactions::needs_confirmation.eq(true))
        .first(&mut conn)
        .unwrap();
    assert_eq!(gen_tx.amount, 220_00);
}

#[actix_web::test]
async fn test_overview_and_cycles_multi_month_parity_and_isolation() {
    use finance_api::cycles::{compute_cycle_summary, compute_overview, list_user_cycles};
    use finance_api::models::{Account, AccountTransfer, CycleConfig, Tag, TransactionTag, TransferTag, Type};
    use finance_api::schema::{account_transfers, accounts, cycle_configs, tags, transaction_tags, transfer_tags};
    use std::time::Instant;
    use time::{Date, Month, PrimitiveDateTime, Time};

    let app = TestApp::new();
    let (user_a, _token_a) = app.create_user("User A", "a@test.local", "secret123");
    let (user_b, _token_b) = app.create_user("User B", "b@test.local", "secret123");

    let mut conn = app.pool.get().unwrap();
    let now = now_utc();

    // Create accounts for A and B
    let acc_a = Account {
        id: Uuid::new_v4().to_string(),
        name: "Conta A".into(),
        icon: None,
        balance: 0,
        bank_name: "BB".into(),
        account_type: "checking".into(),
        user_id: user_a.id.clone(),
        deleted_at: None,
        created_at: now,
        updated_at: now,
    };
    let acc_inv_a = Account {
        id: Uuid::new_v4().to_string(),
        name: "Corretora A".into(),
        icon: None,
        balance: 0,
        bank_name: "XP".into(),
        account_type: "investment".into(),
        user_id: user_a.id.clone(),
        deleted_at: None,
        created_at: now,
        updated_at: now,
    };
    let acc_b = Account {
        id: Uuid::new_v4().to_string(),
        name: "Conta B".into(),
        icon: None,
        balance: 0,
        bank_name: "Nubank".into(),
        account_type: "checking".into(),
        user_id: user_b.id.clone(),
        deleted_at: None,
        created_at: now,
        updated_at: now,
    };
    diesel::insert_into(accounts::table)
        .values(&vec![acc_a.clone(), acc_inv_a.clone(), acc_b.clone()])
        .execute(&mut conn)
        .unwrap();

    let cats_a: Vec<Category> = categories::table
        .filter(categories::user_id.eq(&user_a.id))
        .load(&mut conn)
        .unwrap();
    let salary_cat_a = cats_a.iter().find(|c| c.is_salary).unwrap().clone();
    let expense_cats_a: Vec<Category> = cats_a
        .iter()
        .filter(|c| c.category_type == "expense")
        .cloned()
        .collect();

    let tags_a: Vec<Tag> = tags::table
        .filter(tags::user_id.eq(&user_a.id))
        .load(&mut conn)
        .unwrap();
    let casa_tag_a = tags_a.iter().find(|t| t.name == "casa").unwrap().clone();
    let rf_tag_a = tags_a.iter().find(|t| t.name == "renda_fixa").unwrap().clone();
    let rv_tag_a = tags_a.iter().find(|t| t.name == "renda_variavel").unwrap().clone();

    // Seed 24 months (2024-01 .. 2025-12) for user_a:
    // Each month: 1 salary, 30 expenses (including informational + deleted + casa-tagged), 2 transfers (1 investment, 1 savings/investment)
    // Plus noise data for user_b to ensure strict isolation!
    let mut tx_batch = Vec::new();
    let mut tx_tag_batch = Vec::new();
    let mut tr_batch = Vec::new();
    let mut tr_tag_batch = Vec::new();

    for m_idx in 0..24u32 {
        let year = 2024 + (m_idx / 12) as i32;
        let month_num = (m_idx % 12) + 1;
        let month = Month::try_from(month_num as u8).unwrap();
        let d_base = Date::from_calendar_date(year, month, 5).unwrap();
        let dt_base = PrimitiveDateTime::new(d_base, Time::from_hms(10, 0, 0).unwrap());

        // Salary
        tx_batch.push(Transaction {
            id: Uuid::new_v4().to_string(),
            amount: 8000_00 + (m_idx as i64) * 100_00,
            description: Some(format!("Salario {year}-{month_num:02}")),
            date: dt_base,
            type_: Type::Income,
            payment_method: "salary_account".into(),
            card_name: None,
            account_id: acc_a.id.clone(),
            transfer_id: None,
            category_id: Some(salary_cat_a.id.clone()),
            is_fixed: Some(true),
            is_informational: false,
            credit_paid: false,
            installment_group_id: None,
            installment_number: None,
            installment_total: None,
            recurring_rule_id: None,
            needs_confirmation: false,
            user_id: user_a.id.clone(),
            deleted_at: None,
            created_at: now,
            updated_at: now,
        });

        // 30 regular expenses
        for e in 0..30u32 {
            let day = ((e % 14) + 1) as u8;
            let d = Date::from_calendar_date(year, month, day).unwrap();
            let dt = PrimitiveDateTime::new(d, Time::from_hms(12, 0, 0).unwrap());
            let cat = &expense_cats_a[(e as usize) % expense_cats_a.len()];
            let tx_id = Uuid::new_v4().to_string();
            let pm = match e % 3 {
                0 => "credit",
                1 => "pix",
                _ => "debit",
            };
            tx_batch.push(Transaction {
                id: tx_id.clone(),
                amount: 100_00 + ((e as i64) * 15_00),
                description: Some(format!("Gasto {m_idx}-{e}")),
                date: dt,
                type_: Type::Expense,
                payment_method: pm.into(),
                card_name: if pm == "credit" { Some("Nubank".into()) } else { None },
                account_id: acc_a.id.clone(),
                transfer_id: None,
                category_id: Some(cat.id.clone()),
                is_fixed: Some(e % 4 == 0),
                is_informational: false,
                credit_paid: false,
                installment_group_id: None,
                installment_number: None,
                installment_total: None,
                recurring_rule_id: None,
                needs_confirmation: false,
                user_id: user_a.id.clone(),
                deleted_at: None,
                created_at: now,
                updated_at: now,
            });
            if e % 5 == 0 {
                tx_tag_batch.push(TransactionTag {
                    transaction_id: tx_id,
                    tag_id: casa_tag_a.id.clone(),
                });
            }
        }

        // 1 informational expense (must NOT count in expenses)
        tx_batch.push(Transaction {
            id: Uuid::new_v4().to_string(),
            amount: 9999_00,
            description: Some("Pagamento fatura informativo".into()),
            date: dt_base,
            type_: Type::Expense,
            payment_method: "pix".into(),
            card_name: None,
            account_id: acc_a.id.clone(),
            transfer_id: None,
            category_id: None,
            is_fixed: None,
            is_informational: true,
            credit_paid: false,
            installment_group_id: None,
            installment_number: None,
            installment_total: None,
            recurring_rule_id: None,
            needs_confirmation: false,
            user_id: user_a.id.clone(),
            deleted_at: None,
            created_at: now,
            updated_at: now,
        });

        // 1 soft-deleted expense (must NOT count in expenses)
        tx_batch.push(Transaction {
            id: Uuid::new_v4().to_string(),
            amount: 8888_00,
            description: Some("Deletado".into()),
            date: dt_base,
            type_: Type::Expense,
            payment_method: "pix".into(),
            card_name: None,
            account_id: acc_a.id.clone(),
            transfer_id: None,
            category_id: None,
            is_fixed: None,
            is_informational: false,
            credit_paid: false,
            installment_group_id: None,
            installment_number: None,
            installment_total: None,
            recurring_rule_id: None,
            needs_confirmation: false,
            user_id: user_a.id.clone(),
            deleted_at: Some(now),
            created_at: now,
            updated_at: now,
        });

        // 1 User B expense (must NOT leak into User A)
        tx_batch.push(Transaction {
            id: Uuid::new_v4().to_string(),
            amount: 7777_00,
            description: Some("Outro usuario".into()),
            date: dt_base,
            type_: Type::Expense,
            payment_method: "pix".into(),
            card_name: None,
            account_id: acc_b.id.clone(),
            transfer_id: None,
            category_id: None,
            is_fixed: None,
            is_informational: false,
            credit_paid: false,
            installment_group_id: None,
            installment_number: None,
            installment_total: None,
            recurring_rule_id: None,
            needs_confirmation: false,
            user_id: user_b.id.clone(),
            deleted_at: None,
            created_at: now,
            updated_at: now,
        });

        // Transfers for user_a: investment (RF / RV / untagged other) + savings transfer on m_idx == 2
        let tr1_id = Uuid::new_v4().to_string();
        tr_batch.push(AccountTransfer {
            id: tr1_id.clone(),
            from_account_id: acc_a.id.clone(),
            to_account_id: Some(acc_inv_a.id.clone()),
            amount: 500_00,
            description: Some("Aporte".into()),
            date: dt_base,
            transfer_kind: "investment".into(),
            user_id: user_a.id.clone(),
            deleted_at: None,
            created_at: now,
            updated_at: now,
        });
        if m_idx % 3 != 2 {
            tr_tag_batch.push(TransferTag {
                transfer_id: tr1_id,
                tag_id: if m_idx % 3 == 0 {
                    rf_tag_a.id.clone()
                } else {
                    rv_tag_a.id.clone()
                },
            });
        }
        if m_idx == 2 {
            tr_batch.push(AccountTransfer {
                id: Uuid::new_v4().to_string(),
                from_account_id: acc_a.id.clone(),
                to_account_id: Some(acc_inv_a.id.clone()),
                amount: 1234_00,
                description: Some("Reserva via transferencia".into()),
                date: dt_base,
                transfer_kind: "savings".into(),
                user_id: user_a.id.clone(),
                deleted_at: None,
                created_at: now,
                updated_at: now,
            });
        }

        // Soft-deleted transfer (must NOT count)
        tr_batch.push(AccountTransfer {
            id: Uuid::new_v4().to_string(),
            from_account_id: acc_a.id.clone(),
            to_account_id: Some(acc_inv_a.id.clone()),
            amount: 4444_00,
            description: Some("Transfer deletada".into()),
            date: dt_base,
            transfer_kind: "investment".into(),
            user_id: user_a.id.clone(),
            deleted_at: Some(now),
            created_at: now,
            updated_at: now,
        });
    }

    // Custom CycleConfig with fixed savings for 2024-06 and custom bounds for 2025-01
    diesel::insert_into(cycle_configs::table)
        .values(&vec![
            CycleConfig {
                id: Uuid::new_v4().to_string(),
                user_id: user_a.id.clone(),
                cycle_key: "2024-06".into(),
                start_date: "2024-05-20".into(),
                end_date: "2024-06-19".into(),
                label: Some("Financeiro - Junho 2024".into()),
                savings_mode: "fixed".into(),
                savings_percent: None,
                savings_fixed_amount: Some(2500_00),
                notes: None,
                created_at: now,
                updated_at: now,
            },
            CycleConfig {
                id: Uuid::new_v4().to_string(),
                user_id: user_a.id.clone(),
                cycle_key: "2025-01".into(),
                start_date: "2024-12-20".into(),
                end_date: "2025-01-19".into(),
                label: Some("Janeiro 2025".into()),
                savings_mode: "percent".into(),
                savings_percent: Some(40.0),
                savings_fixed_amount: None,
                notes: None,
                created_at: now,
                updated_at: now,
            },
        ])
        .execute(&mut conn)
        .unwrap();

    // Edge cases for 2024-06 (2024-05-20 00:00:00 ..= 2024-06-19 23:59:59):
    // 1. Exact start/end boundary expenses (must both be included)
    let bound_start_tx_id = Uuid::new_v4().to_string();
    let bound_end_tx_id = Uuid::new_v4().to_string();
    tx_batch.push(Transaction {
        id: bound_start_tx_id.clone(),
        amount: 33_00,
        description: Some("Borda inicio".into()),
        date: PrimitiveDateTime::new(
            Date::from_calendar_date(2024, Month::May, 20).unwrap(),
            Time::MIDNIGHT,
        ),
        type_: Type::Expense,
        payment_method: "pix".into(),
        card_name: None,
        account_id: acc_a.id.clone(),
        transfer_id: None,
        category_id: Some(expense_cats_a[0].id.clone()),
        is_fixed: None,
        is_informational: false,
        credit_paid: false,
        installment_group_id: None,
        installment_number: None,
        installment_total: None,
        recurring_rule_id: None,
        needs_confirmation: false,
        user_id: user_a.id.clone(),
        deleted_at: None,
        created_at: now,
        updated_at: now,
    });
    tx_batch.push(Transaction {
        id: bound_end_tx_id,
        amount: 44_00,
        description: Some("Borda fim".into()),
        date: PrimitiveDateTime::new(
            Date::from_calendar_date(2024, Month::June, 19).unwrap(),
            Time::from_hms(23, 59, 59).unwrap(),
        ),
        type_: Type::Expense,
        payment_method: "debit".into(),
        card_name: None,
        account_id: acc_a.id.clone(),
        transfer_id: None,
        category_id: Some(expense_cats_a[0].id.clone()),
        is_fixed: None,
        is_informational: false,
        credit_paid: false,
        installment_group_id: None,
        installment_number: None,
        installment_total: None,
        recurring_rule_id: None,
        needs_confirmation: false,
        user_id: user_a.id.clone(),
        deleted_at: None,
        created_at: now,
        updated_at: now,
    });
    // 2. Zero and negative expenses (must be ignored by both list_user_cycles & compute_overview)
    for bad_amt in [0i64, -500_00i64] {
        tx_batch.push(Transaction {
            id: Uuid::new_v4().to_string(),
            amount: bad_amt,
            description: Some("Valor nao positivo".into()),
            date: PrimitiveDateTime::new(
                Date::from_calendar_date(2024, Month::June, 10).unwrap(),
                Time::from_hms(12, 0, 0).unwrap(),
            ),
            type_: Type::Expense,
            payment_method: "pix".into(),
            card_name: None,
            account_id: acc_a.id.clone(),
            transfer_id: None,
            category_id: Some(expense_cats_a[0].id.clone()),
            is_fixed: None,
            is_informational: false,
            credit_paid: false,
            installment_group_id: None,
            installment_number: None,
            installment_total: None,
            recurring_rule_id: None,
            needs_confirmation: false,
            user_id: user_a.id.clone(),
            deleted_at: None,
            created_at: now,
            updated_at: now,
        });
    }
    // 3. Soft-deleted "casa" tag linked to bound_start_tx_id must NOT be counted as casa
    let deleted_casa_tag = Tag {
        id: Uuid::new_v4().to_string(),
        name: "CASA".into(),
        color: None,
        user_id: user_a.id.clone(),
        deleted_at: Some(now),
        created_at: now,
        updated_at: now,
    };
    diesel::insert_into(tags::table)
        .values(&deleted_casa_tag)
        .execute(&mut conn)
        .unwrap();
    tx_tag_batch.push(TransactionTag {
        transaction_id: bound_start_tx_id,
        tag_id: deleted_casa_tag.id,
    });

    for chunk in tx_batch.chunks(100) {
        diesel::insert_into(transactions::table).values(chunk).execute(&mut conn).unwrap();
    }
    for chunk in tx_tag_batch.chunks(100) {
        diesel::insert_into(transaction_tags::table).values(chunk).execute(&mut conn).unwrap();
    }
    for chunk in tr_batch.chunks(100) {
        diesel::insert_into(account_transfers::table).values(chunk).execute(&mut conn).unwrap();
    }
    for chunk in tr_tag_batch.chunks(100) {
        diesel::insert_into(transfer_tags::table).values(chunk).execute(&mut conn).unwrap();
    }

    // Measure list_user_cycles and compute_overview across 10 iterations
    let t_list = Instant::now();
    let mut cycles = Vec::new();
    for _ in 0..10 {
        cycles = list_user_cycles(&mut conn, &user_a).unwrap();
    }
    let list_avg_ms = t_list.elapsed().as_secs_f64() * 1000.0 / 10.0;

    let t_ov = Instant::now();
    let mut ov = compute_overview(&mut conn, &user_a).unwrap();
    for _ in 0..9 {
        ov = compute_overview(&mut conn, &user_a).unwrap();
    }
    let ov_avg_ms = t_ov.elapsed().as_secs_f64() * 1000.0 / 10.0;

    eprintln!(
        "[BENCH] cycles={} | list_user_cycles avg={:.3}ms | compute_overview avg={:.3}ms",
        cycles.len(),
        list_avg_ms,
        ov_avg_ms
    );

    // Verify parity against individual compute_cycle_summary calls
    assert!(cycles.len() >= 24);
    assert_eq!(ov.monthly_evolution.len(), cycles.len());
    assert_eq!(ov.kpis.months_count, cycles.len());

    let mut expected_income = 0i64;
    let mut expected_expenses = 0i64;
    let mut expected_saved_inv = 0i64;
    let mut expected_casa = 0i64;
    for (idx, c) in cycles.iter().enumerate() {
        let s = compute_cycle_summary(&mut conn, &user_a, &c.cycle_key, false, false).unwrap();
        assert_eq!(
            c.total_expenses, s.total_expenses,
            "CycleInfo.total_expenses mismatch for {}",
            c.cycle_key
        );
        let pt = &ov.monthly_evolution[idx];
        assert_eq!(pt.cycle_key, s.cycle_key);
        assert_eq!(pt.label, s.label);
        assert_eq!(pt.income, s.total_income);
        assert_eq!(pt.expenses, s.total_expenses);
        assert_eq!(pt.saved, s.saved_amount);
        assert_eq!(pt.free_remainder, s.free_remainder);

        let inv = &ov.investments_savings[idx];
        assert_eq!(inv.saved, s.saved_amount);
        assert_eq!(inv.renda_fixa, s.investments_renda_fixa);
        assert_eq!(inv.renda_variavel, s.investments_renda_variavel);
        assert_eq!(inv.other_investments, s.investments_other);

        let trend = &ov.categories_trend[idx];
        assert_eq!(trend.cycle_key, s.cycle_key);
        for cat_name in &ov.top5_categories {
            let expected_cat_val = s
                .by_category
                .iter()
                .find(|bc| &bc.name == cat_name)
                .map(|bc| bc.total)
                .unwrap_or(0);
            assert_eq!(
                trend.totals.get(cat_name).copied().unwrap_or(0),
                expected_cat_val
            );
        }

        expected_income += s.total_income;
        expected_expenses += s.total_expenses;
        expected_saved_inv += s.total_saved_and_invested;
        expected_casa += s.casa_total;
    }

    assert_eq!(ov.kpis.total_income, expected_income);
    assert_eq!(ov.kpis.total_consumption, expected_expenses);
    assert_eq!(ov.kpis.total_saved_and_invested, expected_saved_inv);
    assert_eq!(ov.kpis.total_casa, expected_casa);
    assert_eq!(ov.kpis.months_with_spending, 24);
    // Verify custom fixed savings (2024-06 = 2500_00) and percent override (2025-01 = 40%)
    let jun24 = ov.monthly_evolution.iter().find(|p| p.cycle_key == "2024-06").unwrap();
    assert_eq!(jun24.saved, 2500_00);
    let jan25 = ov.monthly_evolution.iter().find(|p| p.cycle_key == "2025-01").unwrap();
    assert_eq!(jan25.saved, ((jan25.income as f64) * 0.40).round() as i64);
    // Verify clean_cycle_label stripped "Financeiro - "
    assert_eq!(jun24.label, "Junho 2024");

    // Verify HTTP serialization of /api/cycles includes total_expenses and /api/overview works
    let srv = init_test_svc!(app, LoginRateLimiter::new());
    let req = test::TestRequest::get()
        .uri("/api/cycles")
        .insert_header(("Authorization", format!("Bearer {}", _token_a)))
        .to_request();
    let http_cycles: Vec<Value> = test::call_and_read_body_json(&srv, req).await;
    assert_eq!(http_cycles.len(), cycles.len());
    let http_jun24 = http_cycles
        .iter()
        .find(|c| c["cycle_key"].as_str() == Some("2024-06"))
        .unwrap();
    assert_eq!(http_jun24["total_expenses"].as_i64(), Some(jun24.expenses));
    assert_eq!(http_jun24["label"].as_str(), Some("Junho 2024"));

    let req = test::TestRequest::get()
        .uri("/api/overview")
        .insert_header(("Authorization", format!("Bearer {}", _token_a)))
        .to_request();
    let http_ov: Value = test::call_and_read_body_json(&srv, req).await;
    assert_eq!(http_ov["kpis"]["total_income"].as_i64(), Some(ov.kpis.total_income));
    assert_eq!(
        http_ov["kpis"]["total_consumption"].as_i64(),
        Some(ov.kpis.total_consumption)
    );
    assert_eq!(
        http_ov["kpis"]["total_casa"].as_i64(),
        Some(ov.kpis.total_casa)
    );
    assert_eq!(
        http_ov["monthly_evolution"].as_array().map(|a| a.len()),
        Some(cycles.len())
    );

    // Verify User B only sees their own expenses (isolated from User A)
    let cycles_b = list_user_cycles(&mut conn, &user_b).unwrap();
    let jun24_b = cycles_b.iter().find(|c| c.cycle_key == "2024-06").unwrap();
    assert_eq!(jun24_b.total_expenses, 7777_00);
    let ov_b = compute_overview(&mut conn, &user_b).unwrap();
    assert_eq!(ov_b.kpis.total_consumption, 24 * 7777_00);

    // Verify empty user edge case (0 txs, 0 transfers)
    let (user_c, _token_c) = app.create_user("User C", "c@test.local", "secret123");
    let cycles_c = list_user_cycles(&mut conn, &user_c).unwrap();
    assert_eq!(cycles_c.len(), 1);
    assert_eq!(cycles_c[0].total_expenses, 0);
    let ov_c = compute_overview(&mut conn, &user_c).unwrap();
    assert_eq!(ov_c.kpis.total_income, 0);
    assert_eq!(ov_c.kpis.total_consumption, 0);
    assert_eq!(ov_c.kpis.months_count, 1);
    assert_eq!(ov_c.kpis.months_with_spending, 0);
}
