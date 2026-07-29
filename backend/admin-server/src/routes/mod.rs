//! AdminServerのHTTPハンドラー。

pub(crate) mod assets;
pub(crate) mod controls;
pub(crate) mod registry;

use std::time::{SystemTime, UNIX_EPOCH};

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;

#[derive(Serialize)]
pub(crate) struct Health {
    status: &'static str,
    unix_time: u64,
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

pub(crate) async fn health() -> Json<Health> {
    Json(Health {
        status: "ok",
        unix_time: unix_time(),
    })
}

pub(crate) fn error(status: StatusCode, message: &str) -> Response {
    (
        status,
        Json(ErrorBody {
            error: message.into(),
        }),
    )
        .into_response()
}

fn unix_time() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
