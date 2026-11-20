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
