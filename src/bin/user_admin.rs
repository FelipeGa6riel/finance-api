use diesel::prelude::*;
use dotenvy::dotenv;
use finance_api::db::{build_pool, now_utc};
use finance_api::defaults::seed_user_defaults;
use finance_api::models::User;
use finance_api::schema::users;
use std::env;
use uuid::Uuid;

fn get_flag(args: &[String], flag: &str) -> Option<String> {
    args.windows(2)
        .find(|w| w[0] == flag)
        .map(|w| w[1].clone())
}

fn print_usage() {
    eprintln!("Uso: user_admin <subcomando> [opções]");
    eprintln!("Subcomandos:");
    eprintln!("  create --name <NOME> --email <EMAIL> --password <SENHA> [--cycle-start-day 20] [--savings-percent 35]");
    eprintln!("  list");
    eprintln!("  deactivate --email <EMAIL>");
    eprintln!("  reactivate --email <EMAIL>");
    eprintln!("  reset-password --email <EMAIL> --password <NOVA_SENHA>");
}

fn main() {
    dotenv().ok();
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        print_usage();
        std::process::exit(1);
    }

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = build_pool(&database_url);
    let mut conn = pool.get().expect("Failed to connect to database");
    let cmd = args[1].as_str();

    match cmd {
        "create" => {
            let name = get_flag(&args, "--name").unwrap_or_else(|| {
                eprintln!("Faltou --name");
                std::process::exit(1);
            });
            let email = get_flag(&args, "--email")
                .unwrap_or_else(|| {
                    eprintln!("Faltou --email");
                    std::process::exit(1);
                })
                .trim()
                .to_lowercase();
            let password = get_flag(&args, "--password").unwrap_or_else(|| {
                eprintln!("Faltou --password");
                std::process::exit(1);
            });
            if password.len() < 6 {
                eprintln!("A senha deve ter pelo menos 6 caracteres.");
                std::process::exit(1);
            }
            let cycle_start_day: i32 = get_flag(&args, "--cycle-start-day")
                .and_then(|s| s.parse().ok())
                .unwrap_or(20);
            if !(1..=28).contains(&cycle_start_day) {
                eprintln!("cycle-start-day deve estar entre 1 e 28");
                std::process::exit(1);
            }
            let savings_percent: f32 = get_flag(&args, "--savings-percent")
                .and_then(|s| s.parse().ok())
                .unwrap_or(35.0);

            let existing = users::table
                .filter(users::email.eq(&email))
                .first::<User>(&mut conn)
                .optional()
                .expect("Falha ao consultar email");
            if existing.is_some() {
                eprintln!("Já existe usuário com o email {email}");
                std::process::exit(1);
            }

            let hashed = bcrypt::hash(&password, bcrypt::DEFAULT_COST).expect("Erro bcrypt");
            let now = now_utc();
            let new_user = User {
                id: Uuid::new_v4().to_string(),
                name: name.trim().to_string(),
                currency: Some("BRL".to_string()),
                email,
                password: Some(hashed),
                cycle_start_day,
                default_savings_percent: savings_percent,
                deleted_at: None,
                created_at: now,
                updated_at: now,
            };

            conn.transaction::<_, diesel::result::Error, _>(|conn| {
                diesel::insert_into(users::table)
                    .values(&new_user)
                    .execute(conn)?;
                seed_user_defaults(conn, &new_user.id)?;
                Ok(())
            })
            .expect("Erro ao criar usuário");

            println!(
                "Usuário criado com sucesso: id={} email={} cycle_start_day={}",
                new_user.id, new_user.email, new_user.cycle_start_day
            );
        }
        "list" => {
            let all = users::table
                .order(users::created_at.asc())
                .load::<User>(&mut conn)
                .expect("Erro ao listar usuários");
            println!("{:<38} {:<26} {:<22} {:<6} {}", "ID", "EMAIL", "NOME", "CICLO", "STATUS");
            for u in all {
                let status = if u.deleted_at.is_some() {
                    "INATIVO (soft-deleted)"
                } else {
                    "ATIVO"
                };
                println!(
                    "{:<38} {:<26} {:<22} {:<6} {}",
                    u.id, u.email, u.name, u.cycle_start_day, status
                );
            }
        }
        "deactivate" => {
            let email = get_flag(&args, "--email")
                .unwrap_or_else(|| {
                    eprintln!("Faltou --email");
                    std::process::exit(1);
                })
                .trim()
                .to_lowercase();
            let now = now_utc();
            let count = diesel::update(users::table.filter(users::email.eq(&email)))
                .set((users::deleted_at.eq(Some(now)), users::updated_at.eq(now)))
                .execute(&mut conn)
                .expect("Erro ao desativar");
            if count == 0 {
                eprintln!("Usuário não encontrado: {email}");
                std::process::exit(1);
            }
            println!("Usuário {email} desativado (soft delete).");
        }
        "reactivate" => {
            let email = get_flag(&args, "--email")
                .unwrap_or_else(|| {
                    eprintln!("Faltou --email");
                    std::process::exit(1);
                })
                .trim()
                .to_lowercase();
            let count = diesel::update(users::table.filter(users::email.eq(&email)))
                .set((users::deleted_at.eq(None::<time::PrimitiveDateTime>), users::updated_at.eq(now_utc())))
                .execute(&mut conn)
                .expect("Erro ao reativar");
            if count == 0 {
                eprintln!("Usuário não encontrado: {email}");
                std::process::exit(1);
            }
            println!("Usuário {email} reativado.");
        }
        "reset-password" => {
            let email = get_flag(&args, "--email")
                .unwrap_or_else(|| {
                    eprintln!("Faltou --email");
                    std::process::exit(1);
                })
                .trim()
                .to_lowercase();
            let password = get_flag(&args, "--password").unwrap_or_else(|| {
                eprintln!("Faltou --password");
                std::process::exit(1);
            });
            if password.len() < 6 {
                eprintln!("A senha deve ter pelo menos 6 caracteres.");
                std::process::exit(1);
            }
            let hashed = bcrypt::hash(&password, bcrypt::DEFAULT_COST).expect("Erro bcrypt");
            let count = diesel::update(users::table.filter(users::email.eq(&email)))
                .set((users::password.eq(Some(hashed)), users::updated_at.eq(now_utc())))
                .execute(&mut conn)
                .expect("Erro ao redefinir senha");
            if count == 0 {
                eprintln!("Usuário não encontrado: {email}");
                std::process::exit(1);
            }
            println!("Senha de {email} redefinida com sucesso.");
        }
        _ => {
            print_usage();
            std::process::exit(1);
        }
    }
}
