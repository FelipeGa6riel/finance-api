use actix_cors::Cors;
use actix_web::{App, HttpServer, middleware::Logger, web};
use dotenvy::dotenv;
use finance_api::auth::{AuthConfig, LoginRateLimiter, get_secret_key};
use finance_api::configure_app;
use finance_api::db::build_pool;
use std::env;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv().ok();
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let secret_key = get_secret_key();
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let bind_addr = env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8007".to_string());
    let cors_origins_raw = env::var("CORS_ORIGINS").unwrap_or_else(|_| "*".to_string());
    let allowed_origins: Vec<String> = cors_origins_raw
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && s != "*")
        .collect();

    let pool = build_pool(&database_url);
    let app_data = web::Data::new(pool);
    let auth_cfg = web::Data::new(AuthConfig { secret_key });
    let rate_limiter = web::Data::new(LoginRateLimiter::new());

    println!("Starting finance-api on {bind_addr}");

    HttpServer::new(move || {
        let mut cors = Cors::default()
            .allow_any_header()
            .allow_any_method()
            .max_age(3600);
        if allowed_origins.is_empty() {
            cors = cors.allow_any_origin();
        } else {
            for origin in &allowed_origins {
                cors = cors.allowed_origin(origin);
            }
        }

        App::new()
            .wrap(Logger::new(
                "%a %t \"%r\" \x1b[36m%s\x1b[0m %b \"%{Referer}i\" \"%{User-Agent}i\" %T",
            ))
            .wrap(cors)
            .app_data(app_data.clone())
            .app_data(auth_cfg.clone())
            .app_data(rate_limiter.clone())
            .configure(configure_app)
    })
    .bind(bind_addr)?
    .run()
    .await
}
