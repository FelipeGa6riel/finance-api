use crate::services::login;

use self::services::{
    Claims, create_account, create_category, create_transaction, create_user, get_account,
    get_accounts, get_categories, get_category, get_transaction, get_transactions, get_user,
    get_users, update_category, update_transaction, update_user,
};
use actix_cors::Cors;
use actix_web::HttpMessage;
use actix_web::{App, HttpServer, dev::ServiceRequest, middleware::Logger, web};
use actix_web_httpauth::{extractors::bearer::BearerAuth, middleware::HttpAuthentication};
use diesel::SqliteConnection;
use diesel::r2d2::{ConnectionManager, Pool};
use dotenvy::dotenv;
use jsonwebtoken::{DecodingKey, Validation, decode};
use std::env;
mod models;
mod schema;
mod services;

pub async fn validate_user_credentials(
    req: ServiceRequest,
    credentials: BearerAuth,
) -> Result<ServiceRequest, (actix_web::Error, ServiceRequest)> {
    let secret_key = std::env::var("SECRET_KEY").unwrap_or_else(|_| "default_secret".to_string());
    let token = credentials.token();

    let validation = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret_key.as_ref()),
        &Validation::default(),
    );

    match validation {
        Ok(_token_data) => {
            req.extensions_mut().insert(_token_data.claims);
            Ok(req)
        }
        Err(_) => Err((actix_web::error::ErrorUnauthorized("Invalid token"), req)),
    }
}

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
        let auth = HttpAuthentication::bearer(validate_user_credentials);

        App::new()
            .wrap(Logger::new(
                "%a %t \"%r\" \x1b[36m%s\x1b[0m %b \"%{Referer}i\" \"%{User-Agent}i\" %T",
            ))
            .app_data(app_data.clone())
            .wrap(
                Cors::default()
                    .allow_any_header()
                    .allow_any_method()
                    .allow_any_origin(),
            )
            .service(login)
            .service(create_user)
            .service(
                web::scope("/api")
                    .wrap(auth)
                    .service(get_account)
                    .service(get_accounts)
                    .service(get_transaction)
                    .service(get_transactions)
                    .service(get_categories)
                    .service(get_category)
                    .service(get_users)
                    .service(get_user)
                    .service(create_category)
                    .service(update_category)
                    .service(create_account)
                    .service(create_transaction)
                    .service(update_user)
                    .service(update_transaction),
            )
    })
    .bind(("localhost", 8007))?
    .run()
    .await
}
