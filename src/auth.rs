use crate::error::AppError;
use axum::{extract::FromRequestParts, http::request::Parts};
use uuid::Uuid;

pub struct CurrentUser {
    pub id: Uuid,
}

impl<S> FromRequestParts<S> for CurrentUser
where
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let user_id_str = parts
            .headers
            .get("x-user-id")
            .and_then(|val| val.to_str().ok())
            .ok_or_else(|| AppError::Forbidden("Missing x-user-id header".to_string()))?;

        let id = Uuid::parse_str(user_id_str)
            .map_err(|_| AppError::BadRequest("Invalid x-user-id format".to_string()))?;

        Ok(CurrentUser { id })
    }
}
