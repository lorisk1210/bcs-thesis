use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use tokio::task::JoinError;

#[derive(Debug)]
pub struct ViewerError {
    status: StatusCode,
    title: &'static str,
    message: String,
}

pub type ViewerResult<T> = Result<T, ViewerError>;

impl ViewerError {
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            title: "Bad Request",
            message: message.into(),
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            title: "Not Found",
            message: message.into(),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            title: "Internal Error",
            message: message.into(),
        }
    }

    pub fn from_join(error: JoinError) -> Self {
        Self::internal(format!("background task failed: {error}"))
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl IntoResponse for ViewerError {
    fn into_response(self) -> Response {
        let body = crate::html::render_error_page(self.title, &self.message, self.status);
        (self.status, Html(body)).into_response()
    }
}
