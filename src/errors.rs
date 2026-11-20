use actix_web::{HttpResponse, ResponseError, http::StatusCode};
use diesel::result::{DatabaseErrorKind, Error as DieselError};
use std::fmt;

#[derive(Debug)]
pub enum ApiError {
    BadRequest(String),
    Unauthorized(String),
    NotFound(String),
    Conflict(String),
    TooManyRequests(String),
    Internal(String),
}

pub type ApiResult<T> = Result<T, ApiError>;

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApiError::BadRequest(msg)
            | ApiError::Unauthorized(msg)
            | ApiError::NotFound(msg)
            | ApiError::Conflict(msg)
            | ApiError::TooManyRequests(msg)
            | ApiError::Internal(msg) => write!(f, "{msg}"),
        }
    }
}

impl ResponseError for ApiError {
    fn status_code(&self) -> StatusCode {
        match self {
            ApiError::BadRequest(_) => StatusCode::BAD_REQUEST,
            ApiError::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            ApiError::NotFound(_) => StatusCode::NOT_FOUND,
            ApiError::Conflict(_) => StatusCode::CONFLICT,
            ApiError::TooManyRequests(_) => StatusCode::TOO_MANY_REQUESTS,
            ApiError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn error_response(&self) -> HttpResponse {
        HttpResponse::build(self.status_code()).json(serde_json::json!({
            "error": self.to_string()
        }))
    }
}

impl From<DieselError> for ApiError {
    fn from(err: DieselError) -> Self {
        match err {
            DieselError::NotFound => ApiError::NotFound("Recurso não encontrado".into()),
            DieselError::DatabaseError(DatabaseErrorKind::UniqueViolation, info) => {
                ApiError::Conflict(format!("Registro duplicado: {}", info.message()))
            }
            DieselError::DatabaseError(DatabaseErrorKind::ForeignKeyViolation, info) => {
                ApiError::BadRequest(format!("Referência inválida: {}", info.message()))
            }
            other => ApiError::Internal(format!("Erro de banco de dados: {other}")),
        }
    }
}

impl From<diesel::r2d2::PoolError> for ApiError {
    fn from(err: diesel::r2d2::PoolError) -> Self {
        ApiError::Internal(format!("Falha no pool de conexões: {err}"))
    }
}
