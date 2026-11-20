pub mod auth;
pub mod cycles;
pub mod db;
pub mod defaults;
pub mod errors;
pub mod models;
pub mod schema;
pub mod services;

use actix_web::web;
use actix_web_httpauth::middleware::HttpAuthentication;

pub fn configure_app(cfg: &mut web::ServiceConfig) {
    let auth = HttpAuthentication::bearer(auth::validate_user_credentials);

    cfg.service(services::users::login).service(
        web::scope("/api")
            .wrap(auth)
            .service(services::users::get_me)
            .service(services::users::update_me)
            .service(services::users::change_password)
            .service(services::users::get_users)
            .service(services::users::get_user)
            .service(services::users::update_user)
            .service(services::accounts::get_accounts)
            .service(services::accounts::get_account)
            .service(services::accounts::create_account)
            .service(services::accounts::update_account)
            .service(services::accounts::delete_account)
            .service(services::categories::get_categories)
            .service(services::categories::get_category)
            .service(services::categories::create_category)
            .service(services::categories::update_category_put)
            .service(services::categories::update_category_post)
            .service(services::categories::delete_category)
            .service(services::tags::get_tags)
            .service(services::tags::create_tag)
            .service(services::tags::delete_tag)
            .service(services::transactions::get_transactions)
            .service(services::transactions::get_transaction)
            .service(services::transactions::create_transaction)
            .service(services::transactions::update_transaction)
            .service(services::transactions::delete_transaction)
            .service(services::transactions::pay_credit_invoice)
            .service(services::transfers::get_transfers)
            .service(services::transfers::create_transfer)
            .service(services::transfers::delete_transfer)
            .service(services::cycles::get_cycles)
            .service(services::cycles::get_cycle_summary)
            .service(services::cycles::upsert_cycle_config)
            .service(services::cycles::get_overview)
            .service(services::recurrences::get_recurrences)
            .service(services::recurrences::create_recurrence)
            .service(services::recurrences::update_recurrence)
            .service(services::recurrences::delete_recurrence)
            .service(services::recurrences::generate_recurrences),
    );
}
