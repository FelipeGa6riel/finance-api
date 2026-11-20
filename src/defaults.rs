use crate::db::now_utc;
use crate::models::{Category, Tag};
use crate::schema::{categories, tags};
use diesel::prelude::*;
use uuid::Uuid;

struct DefaultCat {
    name: &'static str,
    color: &'static str,
    category_type: &'static str,
    is_fixed: bool,
    is_salary: bool,
}

const DEFAULT_CATEGORIES: &[DefaultCat] = &[
    DefaultCat { name: "Comida", color: "#43a047", category_type: "expense", is_fixed: false, is_salary: false },
    DefaultCat { name: "Transporte", color: "#36a2eb", category_type: "expense", is_fixed: false, is_salary: false },
    DefaultCat { name: "Combustível", color: "#1e88e5", category_type: "expense", is_fixed: false, is_salary: false },
    DefaultCat { name: "Despesa fixa", color: "#607d8b", category_type: "expense", is_fixed: true, is_salary: false },
    DefaultCat { name: "Lazer", color: "#8e24aa", category_type: "expense", is_fixed: false, is_salary: false },
    DefaultCat { name: "Gato", color: "#4bc0c0", category_type: "expense", is_fixed: false, is_salary: false },
    DefaultCat { name: "Casa", color: "#ff5722", category_type: "expense", is_fixed: false, is_salary: false },
    DefaultCat { name: "Cartão", color: "#9966ff", category_type: "expense", is_fixed: false, is_salary: false },
    DefaultCat { name: "Cuidados", color: "#ec407a", category_type: "expense", is_fixed: false, is_salary: false },
    DefaultCat { name: "Carro", color: "#546e7a", category_type: "expense", is_fixed: false, is_salary: false },
    DefaultCat { name: "Saúde", color: "#00acc1", category_type: "expense", is_fixed: false, is_salary: false },
    DefaultCat { name: "Farmácia", color: "#8bc34a", category_type: "expense", is_fixed: false, is_salary: false },
    DefaultCat { name: "Empréstimo", color: "#78909c", category_type: "expense", is_fixed: false, is_salary: false },
    DefaultCat { name: "Mãe", color: "#ff7043", category_type: "expense", is_fixed: false, is_salary: false },
    DefaultCat { name: "Outros", color: "#fb8c00", category_type: "expense", is_fixed: false, is_salary: false },
    DefaultCat { name: "Salário", color: "#006d43", category_type: "income", is_fixed: false, is_salary: true },
    DefaultCat { name: "Devolução", color: "#00a86b", category_type: "income", is_fixed: false, is_salary: false },
    DefaultCat { name: "Empréstimo", color: "#607d8b", category_type: "income", is_fixed: false, is_salary: false },
    DefaultCat { name: "Décimo", color: "#2e7d32", category_type: "income", is_fixed: false, is_salary: false },
    DefaultCat { name: "Venda", color: "#0288d1", category_type: "income", is_fixed: false, is_salary: false },
    DefaultCat { name: "Outros", color: "#fb8c00", category_type: "income", is_fixed: false, is_salary: false },
];

const DEFAULT_TAGS: &[(&str, &str)] = &[
    ("casa", "#ff5722"),
    ("trabalho", "#36a2eb"),
    ("alimento", "#43a047"),
    ("renda_fixa", "#006d43"),
    ("renda_variavel", "#00acc1"),
    ("pc", "#607d8b"),
    ("jogos", "#8e24aa"),
    ("fastfood", "#fb8c00"),
    ("limpeza", "#4bc0c0"),
];

pub fn seed_user_defaults(conn: &mut SqliteConnection, target_user_id: &str) -> QueryResult<()> {
    let now = now_utc();

    let existing_cats: Vec<(String, String)> = categories::table
        .filter(categories::user_id.eq(target_user_id))
        .filter(categories::deleted_at.is_null())
        .select((categories::name, categories::category_type))
        .load(conn)?;

    for cat in DEFAULT_CATEGORIES {
        let already = existing_cats
            .iter()
            .any(|(n, t)| n.eq_ignore_ascii_case(cat.name) && t == cat.category_type);
        if !already {
            let row = Category {
                id: Uuid::new_v4().to_string(),
                name: cat.name.to_string(),
                description: None,
                color: Some(cat.color.to_string()),
                icon: None,
                category_type: cat.category_type.to_string(),
                is_fixed: cat.is_fixed,
                is_salary: cat.is_salary,
                user_id: target_user_id.to_string(),
                deleted_at: None,
                created_at: now,
                updated_at: now,
            };
            diesel::insert_into(categories::table)
                .values(&row)
                .execute(conn)?;
        }
    }

    let existing_tags: Vec<String> = tags::table
        .filter(tags::user_id.eq(target_user_id))
        .filter(tags::deleted_at.is_null())
        .select(tags::name)
        .load(conn)?;

    for (tag_name, color) in DEFAULT_TAGS {
        if !existing_tags.iter().any(|t| t.eq_ignore_ascii_case(tag_name)) {
            let row = Tag {
                id: Uuid::new_v4().to_string(),
                name: (*tag_name).to_string(),
                color: Some((*color).to_string()),
                user_id: target_user_id.to_string(),
                deleted_at: None,
                created_at: now,
                updated_at: now,
            };
            diesel::insert_into(tags::table).values(&row).execute(conn)?;
        }
    }

    Ok(())
}
