use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

/// Every message is one sentence a person can act on; internal detail is logged, never shown.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Nothing was found at this address.")]
    NotFound,
    #[error("Enter the access key in Settings, or send it as a Bearer token.")]
    Unauthorized,
    #[error("That access key is not right. Check it and try again.")]
    WrongKey,
    #[error("Your access does not include this. Enter the access key in Settings.")]
    Forbidden,
    #[error("Too many requests. Wait a minute and try again.")]
    TooManyAttempts,
    #[error("The text has {len} characters; your access allows {limit}. Shorten it and try again.")]
    TooLong { len: usize, limit: usize },
    #[error("That upload is too large. Choose a smaller file.")]
    TooLarge,
    #[error("{0}")]
    Invalid(String),
    #[error("This server has no {0} configured.")]
    NotConfigured(&'static str),
    /// The model endpoint answered with an error or could not be reached; the detail is logged.
    #[error("{message}")]
    Upstream { message: String, detail: String },
    #[error("Something went wrong. Try again later.")]
    Io(#[from] std::io::Error),
    #[error("Something went wrong. Try again later.")]
    Internal(String),
}

impl Error {
    pub fn status(&self) -> StatusCode {
        match self {
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Unauthorized | Self::WrongKey => StatusCode::UNAUTHORIZED,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::TooManyAttempts => StatusCode::TOO_MANY_REQUESTS,
            Self::TooLong { .. } | Self::TooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            Self::Invalid(_) => StatusCode::UNPROCESSABLE_ENTITY,
            Self::NotConfigured(_) => StatusCode::SERVICE_UNAVAILABLE,
            Self::Upstream { .. } => StatusCode::BAD_GATEWAY,
            Self::Io(_) | Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// The stable code API clients branch on.
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotFound => "NOT_FOUND",
            Self::Unauthorized => "UNAUTHORIZED",
            Self::WrongKey => "WRONG_KEY",
            Self::Forbidden => "FORBIDDEN",
            Self::TooManyAttempts => "TOO_MANY_ATTEMPTS",
            Self::TooLong { .. } => "TOO_LONG",
            Self::TooLarge => "TOO_LARGE",
            Self::Invalid(_) => "INVALID",
            Self::NotConfigured(_) => "NOT_CONFIGURED",
            Self::Upstream { .. } => "UPSTREAM",
            Self::Io(_) | Self::Internal(_) => "INTERNAL",
        }
    }

    pub fn internal(e: impl std::fmt::Display) -> Self {
        Self::Internal(e.to_string())
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::Invalid(message.into())
    }

    pub fn upstream(message: impl Into<String>, detail: impl std::fmt::Display) -> Self {
        Self::Upstream { message: message.into(), detail: detail.to_string() }
    }

    /// The detail behind an error's message, for logs and the operator.
    pub fn detail(&self) -> Option<String> {
        match self {
            Self::Io(e) => Some(e.to_string()),
            Self::Internal(d) | Self::Upstream { detail: d, .. } => Some(d.clone()),
            _ => None,
        }
    }

    pub fn startup_message(&self) -> String {
        match self.detail() {
            Some(d) => format!("translation-inference could not start: {}.", d.trim_end_matches('.')),
            None => self.to_string(),
        }
    }

    pub fn log(&self) {
        match self {
            Self::Io(e) => tracing::error!(detail = %e, "io error"),
            Self::Internal(d) => tracing::error!(detail = %d, "internal error"),
            Self::Upstream { detail, .. } => tracing::warn!(detail = %detail, "upstream error"),
            _ => {}
        }
    }
}

/// Carried in the response extensions so the page layer can render the error page and the
/// API layer a JSON body, each from the same error.
#[derive(Debug, Clone)]
pub struct ErrorNotice {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        self.log();
        let notice = ErrorNotice { status: self.status(), code: self.code(), message: self.to_string() };
        let mut res = (notice.status, notice.message.clone()).into_response();
        res.extensions_mut().insert(notice);
        res
    }
}
