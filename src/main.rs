use self::services::{
    create_account, create_category, create_transaction, create_user, get_account, get_accounts,
    get_categories, get_category, get_transaction, get_transactions, get_user, get_users,
    update_category, update_transaction, update_user,
};
use actix_cors::Cors;
use actix_web::{App, HttpServer, middleware::Logger, web};
use diesel::SqliteConnection;
use diesel::r2d2::{ConnectionManager, Pool};
use dotenvy::dotenv;
use std::env;
mod models;
mod schema;
mod services;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv().ok();
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("debug"));

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let manager = ConnectionManager::<SqliteConnection>::new(database_url.clone());

    let pool = Pool::builder()
        .max_size(10)
        .build(manager)
        .unwrap_or_else(|_| panic!("Error building connection pool"));

    let app_data = web::Data::new(pool);

    HttpServer::new(move || {
        App::new()
            .wrap(Logger::default())
            .wrap(
                Cors::default()
                    .allow_any_header()
                    .allow_any_method()
                    .allow_any_origin(),
            )
            .app_data(app_data.clone())
            .service(get_account)
            .service(get_accounts)
            .service(get_transaction)
            .service(get_transactions)
            .service(get_categories)
            .service(get_category)
            .service(get_users)
            .service(get_user)
            .service(create_user)
            .service(create_category)
            .service(update_category)
            .service(create_account)
            .service(create_transaction)
            .service(update_user)
            .service(update_transaction)
    })
    .bind(("localhost", 8007))?
    .run()
    .await
}
