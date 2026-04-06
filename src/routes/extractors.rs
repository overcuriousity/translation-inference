use axum::{
    extract::{
        rejection::JsonRejection, rejection::QueryRejection, FromRequest, FromRequestParts, Query,
        Request,
    },
    http::{request::Parts, StatusCode},
    Json,
};
use serde::de::DeserializeOwned;

use crate::models::ErrorResponse;

fn json_err(msg: String) -> (StatusCode, Json<ErrorResponse>) {
    (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: msg }))
}

/// Wrapper around `axum::Json<T>` that returns a JSON `ErrorResponse` on
/// deserialization failures instead of Axum's default plain-text rejection.
pub struct AppJson<T>(pub T);

#[axum::async_trait]
impl<S, T> FromRequest<S> for AppJson<T>
where
    Json<T>: FromRequest<S, Rejection = JsonRejection>,
    S: Send + Sync,
{
    type Rejection = (StatusCode, Json<ErrorResponse>);

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        match Json::<T>::from_request(req, state).await {
            Ok(Json(value)) => Ok(AppJson(value)),
            Err(rejection) => Err(json_err(rejection.body_text())),
        }
    }
}

/// Wrapper around `axum::extract::Query<T>` that returns a JSON `ErrorResponse`
/// on deserialization failures instead of Axum's default plain-text rejection.
pub struct AppQuery<T>(pub T);

#[axum::async_trait]
impl<S, T> FromRequestParts<S> for AppQuery<T>
where
    Query<T>: FromRequestParts<S, Rejection = QueryRejection>,
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = (StatusCode, Json<ErrorResponse>);

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        match Query::<T>::from_request_parts(parts, state).await {
            Ok(Query(value)) => Ok(AppQuery(value)),
            Err(rejection) => Err(json_err(rejection.body_text())),
        }
    }
}
