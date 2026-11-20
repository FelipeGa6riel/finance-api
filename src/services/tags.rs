use crate::auth::Claims;
use crate::db::{SqlitePool, now_utc};
use crate::errors::{ApiError, ApiResult};
use crate::models::{CreateTagRequest, Tag};
use crate::schema::tags;
use actix_web::{HttpResponse, delete, get, post, web};
use diesel::prelude::*;
use uuid::Uuid;

#[get("/tags")]
pub async fn get_tags(
    pool: web::Data<SqlitePool>,
    claims: web::ReqData<Claims>,
) -> ApiResult<HttpResponse> {
    let mut conn = pool.get()?;
    let list = tags::table
        .filter(tags::user_id.eq(&claims.sub))
        .filter(tags::deleted_at.is_null())
        .order(tags::name.asc())
        .load::<Tag>(&mut conn)?;
    Ok(HttpResponse::Ok().json(list))
}

#[post("/tags")]
pub async fn create_tag(
    payload: web::Json<CreateTagRequest>,
    pool: web::Data<SqlitePool>,
    claims: web::ReqData<Claims>,
) -> ApiResult<HttpResponse> {
    let clean = payload
        .name
        .trim()
        .trim_start_matches('#')
        .to_lowercase();
    if clean.is_empty() {
        return Err(ApiError::BadRequest("Nome da tag é obrigatório".into()));
    }

    let mut conn = pool.get()?;
    let now = now_utc();
    let row = Tag {
        id: Uuid::new_v4().to_string(),
        name: clean,
        color: payload.color.clone(),
        user_id: claims.sub.clone(),
        deleted_at: None,
        created_at: now,
        updated_at: now,
    };

    diesel::insert_into(tags::table).values(&row).execute(&mut conn)?;
    Ok(HttpResponse::Created().json(row))
}

#[delete("/tags/{id}")]
pub async fn delete_tag(
    tag_id: web::Path<String>,
    pool: web::Data<SqlitePool>,
    claims: web::ReqData<Claims>,
) -> ApiResult<HttpResponse> {
    let id = tag_id.into_inner();
    let mut conn = pool.get()?;
    let _existing = tags::table
        .filter(tags::id.eq(&id))
        .filter(tags::user_id.eq(&claims.sub))
        .filter(tags::deleted_at.is_null())
        .first::<Tag>(&mut conn)?;

    let now = now_utc();
    diesel::update(tags::table.filter(tags::id.eq(&id).and(tags::user_id.eq(&claims.sub))))
        .set((tags::deleted_at.eq(Some(now)), tags::updated_at.eq(now)))
        .execute(&mut conn)?;

    Ok(HttpResponse::NoContent().finish())
}
