//! `rest` - HTTP client, ratelimiter, routes.
#![forbid(unsafe_code)]
#![allow(missing_docs)]

pub mod builders;
pub mod client;
pub mod multipart;
pub mod ratelimiter;
pub mod routes;
pub mod send;

pub use builders::{CreateEmbed, CreateMessage, ExecuteWebhook, InteractionCallback};
pub use client::{Client, ClientBuilder};
pub use ratelimiter::{BanMeter, BucketState, Ratelimiter};
pub use routes::{Route, API_VERSION, BASE};

/// HTTP method.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Method {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

impl Method {
    /// As str.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
        }
    }
}
