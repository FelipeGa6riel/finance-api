use crate::auth::Claims;
use crate::db::{SqlitePool, now_utc};
use crate::errors::{ApiError, ApiResult};
use crate::models::{Category, CreateCategoryRequest, UpdateCategoryRequest};
use crate::schema::categories;
use crate::services::ownership::load_owned_category;
use actix_web::{HttpResponse, delete, get, post, put, web};
use diesel::prelude::*;
use uuid::Uuid;

fn normalize_category_type(raw: &str) -> Result<&'static str, String> {
    match raw.trim().to_lowercase().as_str() {
        "expense" | "despesa" => Ok("expense"),
        "income" | "receita" => Ok("income"),
        other => Err(format!("category_type inválido: '{other}'")),
    }
}

#[get("/categories")]
pub async fn get_categories(
    pool: web::Data<SqlitePool>,
    claims: web::ReqData<Claims>,
) -> ApiResult<HttpResponse> {
    let mut conn = pool.get()?;
    let list = categories::table
        .filter(categories::user_id.eq(&claims.sub))
        .filter(categories::deleted_at.is_null())
        .order(categories::name.asc())
        .load::<Category>(&mut conn)?;
    Ok(HttpResponse::Ok().json(list))
}

#[get("/categories/{id}")]
pub async fn get_category(
    category_id: web::Path<String>,
    pool: web::Data<SqlitePool>,
    claims: web::ReqData<Claims>,
) -> ApiResult<HttpResponse> {
    let mut conn = pool.get()?;
    let cat = load_owned_category(&mut conn, &category_id.into_inner(), &claims.sub)?;
    Ok(HttpResponse::Ok().json(cat))
}

#[post("/categories")]
pub async fn create_category(
    payload: web::Json<CreateCategoryRequest>,
    pool: web::Data<SqlitePool>,
    claims: web::ReqData<Claims>,
) -> ApiResult<HttpResponse> {
    let name = payload.name.trim().to_string();
    if name.is_empty() {
        return Err(ApiError::BadRequest("Nome da categoria é obrigatório".into()));
    }
    let cat_type = match &payload.category_type {
        Some(t) => normalize_category_type(t).map_err(ApiError::BadRequest)?,
        None => "expense",
    };

    let mut conn = pool.get()?;
    let now = now_utc();
    let row = Category {
        id: Uuid::new_v4().to_string(),
        name,
        description: payload.description.clone(),
        color: payload.color.clone(),
        icon: payload.icon.clone(),
        category_type: cat_type.to_string(),
        is_fixed: payload.is_fixed.unwrap_or(false),
        is_salary: payload.is_salary.unwrap_or(false),
        user_id: claims.sub.clone(),
        deleted_at: None,
        created_at: now,
        updated_at: now,
    };

    diesel::insert_into(categories::table)
        .values(&row)
        .execute(&mut conn)?;

    Ok(HttpResponse::Created().json(row))
}

fn apply_category_update(
    conn: &mut SqliteConnection,
    id: &str,
    user_id: &str,
    payload: &UpdateCategoryRequest,
) -> ApiResult<Category> {
    let existing = load_owned_category(conn, id, user_id)?;
    let new_name = match &payload.name {
        Some(n) if !n.trim().is_empty() => n.trim().to_string(),
        Some(_) => return Err(ApiError::BadRequest("Nome da categoria não pode ser vazio".into())),
        None => existing.name,
    };
    let new_desc = match &payload.description {
        Some(v) => v.clone(),
        None => existing.description,
    };
    let new_color = match &payload.color {
        Some(v) => v.clone(),
        None => existing.color,
    };
    let new_icon = match &payload.icon {
        Some(v) => v.clone(),
        None => existing.icon,
    };
    let new_type = match &payload.category_type {
        Some(t) => normalize_category_type(t).map_err(ApiError::BadRequest)?.to_string(),
        None => existing.category_type,
    };
    let new_fixed = payload.is_fixed.unwrap_or(existing.is_fixed);
    let new_salary = payload.is_salary.unwrap_or(existing.is_salary);

    diesel::update(
        categories::table.filter(categories::id.eq(id).and(categories::user_id.eq(user_id))),
    )
    .set((
        categories::name.eq(new_name),
        categories::description.eq(new_desc),
        categories::color.eq(new_color),
        categories::icon.eq(new_icon),
        categories::category_type.eq(new_type),
        categories::is_fixed.eq(new_fixed),
        categories::is_salary.eq(new_salary),
        categories::updated_at.eq(now_utc()),
    ))
    .execute(conn)?;

    load_owned_category(conn, id, user_id)
}

#[put("/categories/{id}")]
pub async fn update_category_put(
    category_id: web::Path<String>,
    payload: web::Json<UpdateCategoryRequest>,
    pool: web::Data<SqlitePool>,
    claims: web::ReqData<Claims>,
) -> ApiResult<HttpResponse> {
    let mut conn = pool.get()?;
    let updated = apply_category_update(&mut conn, &category_id.into_inner(), &claims.sub, &payload)?;
    Ok(HttpResponse::Ok().json(updated))
}

#[post("/categories/{id}")]
pub async fn update_category_post(
    category_id: web::Path<String>,
    payload: web::Json<UpdateCategoryRequest>,
    pool: web::Data<SqlitePool>,
    claims: web::ReqData<Claims>,
) -> ApiResult<HttpResponse> {
    let mut conn = pool.get()?;
    let updated = apply_category_update(&mut conn, &category_id.into_inner(), &claims.sub, &payload)?;
    Ok(HttpResponse::Ok().json(updated))
}

#[delete("/categories/{id}")]
pub async fn delete_category(
    category_id: web::Path<String>,
    pool: web::Data<SqlitePool>,
    claims: web::ReqData<Claims>,
) -> ApiResult<HttpResponse> {
    let id = category_id.into_inner();
    let mut conn = pool.get()?;
    let _existing = load_owned_category(&mut conn, &id, &claims.sub)?;
    let now = now_utc();

    diesel::update(
        categories::table.filter(categories::id.eq(&id).and(categories::user_id.eq(&claims.sub))),
    )
    .set((categories::deleted_at.eq(Some(now)), categories::updated_at.eq(now)))
    .execute(&mut conn)?;

    Ok(HttpResponse::NoContent().finish())
}
