//! `rest::Client` - single `reqwest::Client` (rustls-webpki-roots).
use std::fmt;
use std::sync::Mutex;
use std::time::Duration;

/// REST client.
pub struct Client {
    inner: reqwest::Client,
    token: secrecy::SecretString,
    ratelimiter: Mutex<crate::Ratelimiter>,
    base: String,
}

impl fmt::Debug for Client {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Client").field("token", &"***").finish()
    }
}

/// Builder (sync; `build` does no IO).
#[derive(Debug)]
pub struct ClientBuilder {
    token: secrecy::SecretString,
    timeout: Duration,
    base: String,
}

impl ClientBuilder {
    /// Request timeout.
    #[must_use]
    pub fn timeout(mut self, d: Duration) -> Self {
        self.timeout = d;
        self
    }
    /// Override API base (tests point at a mock server; default discord.com).
    #[must_use]
    pub fn base_url(mut self, base: impl Into<String>) -> Self {
        self.base = base.into();
        self
    }
    /// Build (wires `webpki-roots` explicitly; `rustls-tls-manual-roots` ships
    /// no roots alone, so live TLS failed on first call — fixed by using
    /// `rustls-tls-webpki-roots` + explicit opt-in below).
    ///
    /// # Errors
    /// Returns [`common::Error::Config`] when the HTTP client cannot be built.
    pub fn build(self) -> Result<Client, common::Error> {
        let inner = reqwest::Client::builder()
            .timeout(self.timeout)
            .pool_max_idle_per_host(32)
            .tls_built_in_webpki_certs(true)
            .user_agent(format!(
                "DiscordBot (https://github.com/rivulus/rivulus, {})",
                env!("CARGO_PKG_VERSION")
            ))
            .build()
            .map_err(|_| common::Error::Config(Box::from("reqwest build failed")))?;
        Ok(Client {
            inner,
            token: self.token,
            ratelimiter: Mutex::new(crate::Ratelimiter::new()),
            base: self.base,
        })
    }
}

impl Client {
    /// Start building with a token (`SecretString`; never logged).
    pub fn builder(token: impl Into<secrecy::SecretString>) -> ClientBuilder {
        ClientBuilder {
            token: token.into(),
            timeout: Duration::from_secs(15),
            base: crate::BASE.to_owned(),
        }
    }
    /// Full URL for a route (respects builder `base_url` override).
    #[must_use]
    pub fn route_url(&self, route: &crate::Route) -> String {
        route.url_with_base(&self.base)
    }
    /// Authorization header value (kept private; never Debug-printed).
    fn auth(&self) -> String {
        use secrecy::ExposeSecret;
        format!("Bot {}", self.token.expose_secret())
    }
    /// Ratelimiter snapshot hook (P2-send uses it).
    pub(crate) fn ratelimiter_lock(&self) -> std::sync::MutexGuard<'_, crate::Ratelimiter> {
        self.ratelimiter.lock().unwrap_or_else(|e| e.into_inner())
    }
    /// Underlying HTTP client.
    #[must_use]
    pub fn inner(&self) -> &reqwest::Client {
        &self.inner
    }
    /// Pre-emptive wait for a bucket (global + `remaining == 0`).
    async fn wait_for_bucket(&self, bucket: &str) {
        let wait: Option<f64> = self.ratelimiter_lock().should_wait(bucket);
        if let Some(secs) = wait {
            tokio::time::sleep(Duration::from_secs_f64(secs.max(0.0))).await;
        }
    }
    /// Record `X-RateLimit-*` headers for a bucket (Reset-After preferred).
    fn record_headers(&self, bucket: &str, headers: &reqwest::header::HeaderMap) {
        let lim = headers
            .get("x-ratelimit-limit")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok());
        let rem = headers
            .get("x-ratelimit-remaining")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok());
        let ra = headers
            .get("x-ratelimit-reset-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<f64>().ok());
        if let (Some(l), Some(r), Some(a)) = (lim, rem, ra) {
            self.ratelimiter_lock().register_headers(bucket, l, r, a);
        }
    }
    /// Parse `(scope, global-header-present)` from response headers.
    fn scope_and_global(headers: &reqwest::header::HeaderMap) -> (String, bool) {
        let scope: String = headers
            .get("x-ratelimit-scope")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_lowercase();
        let global_hdr = headers.get("x-ratelimit-global").is_some();
        (scope, global_hdr)
    }
    /// Apply bot auth + optional audit-reason to a request builder.
    fn with_auth(
        &self,
        req: reqwest::RequestBuilder,
        audit_reason: Option<&str>,
    ) -> Result<reqwest::RequestBuilder, common::Error> {
        let mut req = req.header(reqwest::header::AUTHORIZATION, self.auth());
        if let Some(r) = audit_reason {
            let enc = common::validate::audit_reason(r)?;
            req = req.header("X-Audit-Log-Reason", enc);
        }
        Ok(req)
    }
}

impl Client {
    /// Execute a JSON request with body bytes (None for GET/DELETE).
    ///
    /// # Errors
    /// Returns typed [`common::Error`] (fail-fast 401/403, `RateLimited` after waits).
    pub async fn execute(
        &self,
        route: &crate::Route,
        body: Option<Vec<u8>>,
        audit_reason: Option<&str>,
    ) -> Result<Vec<u8>, common::Error> {
        tracing::debug!(
            request_id = common::next_request_id(),
            route = route.path_template(),
            "rest execute"
        );
        if let Some(r) = audit_reason {
            common::validate::audit_reason(r)?;
        }
        let bucket = crate::Ratelimiter::bucket_key(route);
        self.wait_for_bucket(&bucket).await;
        let method = match route.method() {
            crate::Method::Get => reqwest::Method::GET,
            crate::Method::Post => reqwest::Method::POST,
            crate::Method::Put => reqwest::Method::PUT,
            crate::Method::Patch => reqwest::Method::PATCH,
            crate::Method::Delete => reqwest::Method::DELETE,
        };
        let url = self.route_url(route);
        let mut attempts: u32 = 0;
        loop {
            attempts += 1;
            let mut req = self.inner.request(method.clone(), url.clone());
            req = self.with_auth(req, audit_reason)?;
            if let Some(b) = &body {
                req = req
                    .header(reqwest::header::CONTENT_TYPE, "application/json")
                    .body(b.clone());
            }
            let resp: reqwest::Response = req.send().await.map_err(|e| {
                if e.is_timeout() {
                    common::Error::Timeout(Box::from("request timeout"))
                } else {
                    common::Error::Network(Box::from("send failed"))
                }
            })?;
            let status = resp.status().as_u16();
            let headers = resp.headers().clone();
            self.record_headers(&bucket, &headers);
            let (_scope, global_hdr) = Self::scope_and_global(&headers);
            if status == 401 {
                return Err(common::Error::Unauthorized);
            }
            if status == 403 {
                return Err(common::Error::Forbidden);
            }
            if status == 429 {
                let bytes = resp.bytes().await.unwrap_or_default();
                #[derive(serde::Deserialize)]
                struct R {
                    #[serde(default)]
                    retry_after: f64,
                    #[serde(default)]
                    global: bool,
                }
                let p: R = serde_json::from_slice(&bytes).unwrap_or(R {
                    retry_after: 1.0,
                    global: global_hdr,
                });
                let global = p.global || global_hdr;
                self.ratelimiter_lock()
                    .retry_after_for_429(p.retry_after, global);
                tokio::time::sleep(Duration::from_secs_f64(p.retry_after.max(0.0))).await;
                if attempts >= 3 {
                    return Err(common::Error::RateLimited {
                        bucket: bucket.clone().into(),
                        retry_after_secs: p.retry_after,
                        global,
                    });
                }
                continue;
            }
            if status == 404 {
                return Err(common::Error::NotFound(Box::from(route.path_template())));
            }
            if (500..600).contains(&status) {
                if crate::send::should_retry(route.method(), status, attempts) {
                    tokio::time::sleep(Duration::from_millis(200 * u64::from(attempts))).await;
                    continue;
                }
                return Err(common::Error::Rest {
                    status,
                    code: 0,
                    message: Box::from("server error"),
                    errors: None,
                });
            }
            if !(200..300).contains(&status) {
                let bytes = resp.bytes().await.unwrap_or_default();
                #[derive(serde::Deserialize)]
                struct D {
                    #[serde(default)]
                    message: String,
                    #[serde(default)]
                    code: i64,
                }
                let p: D = serde_json::from_slice(&bytes).unwrap_or(D {
                    message: String::from("error"),
                    code: 0,
                });
                return Err(common::Error::Rest {
                    status,
                    code: p.code,
                    message: p.message.into(),
                    errors: None,
                });
            }
            let bytes = resp
                .bytes()
                .await
                .map_err(|_| common::Error::Network(Box::from("read failed")))?;
            return Ok(bytes.to_vec());
        }
    }

    /// Execute a multipart request (`payload_json` first + files).
    ///
    /// Reuses [`Self::execute`] header/ratelimit handling; body is a
    /// `multipart/form-data` form built by [`crate::multipart::message_form`].
    ///
    /// # Errors
    /// Returns typed [`common::Error`] (validation, fail-fast 401/403,
    /// `RateLimited` after waits).
    pub async fn execute_multipart(
        &self,
        route: &crate::Route,
        payload_json: &[u8],
        files: &[crate::multipart::MultipartFile],
        audit_reason: Option<&str>,
    ) -> Result<Vec<u8>, common::Error> {
        tracing::debug!(
            request_id = common::next_request_id(),
            route = route.path_template(),
            "rest execute_multipart"
        );
        if let Some(r) = audit_reason {
            common::validate::audit_reason(r)?;
        }
        let form = crate::multipart::message_form(payload_json, files)?;
        let bucket = crate::Ratelimiter::bucket_key(route);
        self.wait_for_bucket(&bucket).await;
        let method = match route.method() {
            crate::Method::Get => reqwest::Method::GET,
            crate::Method::Post => reqwest::Method::POST,
            crate::Method::Put => reqwest::Method::PUT,
            crate::Method::Patch => reqwest::Method::PATCH,
            crate::Method::Delete => reqwest::Method::DELETE,
        };
        let url = self.route_url(route);
        let mut attempts: u32 = 0;
        loop {
            attempts += 1;
            // `Form` is one-shot; rebuild the parts per attempt from inputs.
            let attempt_form = crate::multipart::message_form(payload_json, files)?;
            let _ = &form;
            let mut req = self.inner.request(method.clone(), url.clone());
            req = self.with_auth(req, audit_reason)?;
            req = req.multipart(attempt_form);
            let resp: reqwest::Response = req.send().await.map_err(|e| {
                if e.is_timeout() {
                    common::Error::Timeout(Box::from("request timeout"))
                } else {
                    common::Error::Network(Box::from("send failed"))
                }
            })?;
            let status = resp.status().as_u16();
            let headers = resp.headers().clone();
            self.record_headers(&bucket, &headers);
            let (_scope, global_hdr) = Self::scope_and_global(&headers);
            if status == 401 {
                return Err(common::Error::Unauthorized);
            }
            if status == 403 {
                return Err(common::Error::Forbidden);
            }
            if status == 429 {
                let bytes = resp.bytes().await.unwrap_or_default();
                #[derive(serde::Deserialize)]
                struct R {
                    #[serde(default)]
                    retry_after: f64,
                    #[serde(default)]
                    global: bool,
                }
                let p: R = serde_json::from_slice(&bytes).unwrap_or(R {
                    retry_after: 1.0,
                    global: global_hdr,
                });
                let global = p.global || global_hdr;
                self.ratelimiter_lock()
                    .retry_after_for_429(p.retry_after, global);
                tokio::time::sleep(Duration::from_secs_f64(p.retry_after.max(0.0))).await;
                if attempts >= 3 {
                    return Err(common::Error::RateLimited {
                        bucket: bucket.clone().into(),
                        retry_after_secs: p.retry_after,
                        global,
                    });
                }
                continue;
            }
            if status == 404 {
                return Err(common::Error::NotFound(Box::from(route.path_template())));
            }
            if (500..600).contains(&status) {
                if crate::send::should_retry(route.method(), status, attempts) {
                    tokio::time::sleep(Duration::from_millis(200 * u64::from(attempts))).await;
                    continue;
                }
                return Err(common::Error::Rest {
                    status,
                    code: 0,
                    message: Box::from("server error"),
                    errors: None,
                });
            }
            if !(200..300).contains(&status) {
                let bytes = resp.bytes().await.unwrap_or_default();
                #[derive(serde::Deserialize)]
                struct D {
                    #[serde(default)]
                    message: String,
                    #[serde(default)]
                    code: i64,
                }
                let p: D = serde_json::from_slice(&bytes).unwrap_or(D {
                    message: String::from("error"),
                    code: 0,
                });
                return Err(common::Error::Rest {
                    status,
                    code: p.code,
                    message: p.message.into(),
                    errors: None,
                });
            }
            let bytes = resp
                .bytes()
                .await
                .map_err(|_| common::Error::Network(Box::from("read failed")))?;
            return Ok(bytes.to_vec());
        }
    }

    /// Execute a webhook with a full secret URL (30s timeout; token never logged).
    ///
    /// `url_secret` holds the complete `https://discord.com/api/webhooks/...`
    /// URL (id + token). It is exposed only to build the request URL and never
    /// appears in errors, logs, or `Debug`. `query` is the `?wait&...` string
    /// from [`crate::builders::ExecuteWebhook::query`]; `body` is JSON bytes.
    /// Never retries `401`/`403`/`404` (a `404` means the webhook is dead).
    ///
    /// # Errors
    /// Returns typed [`common::Error`] (fail-fast 401/403/404, `RateLimited`
    /// after waits).
    pub async fn execute_webhook(
        &self,
        url_secret: &secrecy::SecretString,
        query: Option<&str>,
        body: Option<Vec<u8>>,
        audit_reason: Option<&str>,
    ) -> Result<Vec<u8>, common::Error> {
        use secrecy::ExposeSecret;
        tracing::debug!(
            request_id = common::next_request_id(),
            bucket = "POST:/webhooks/execute",
            "rest execute_webhook"
        );
        if let Some(r) = audit_reason {
            common::validate::audit_reason(r)?;
        }
        let bucket = String::from("POST:/webhooks/execute");
        self.wait_for_bucket(&bucket).await;
        let full_url = match query {
            Some(q) if !q.is_empty() => format!("{}{}", url_secret.expose_secret(), q),
            _ => url_secret.expose_secret().clone(),
        };
        let mut attempts: u32 = 0;
        loop {
            attempts += 1;
            let mut req = self
                .inner
                .request(reqwest::Method::POST, full_url.clone())
                .timeout(Duration::from_secs(30));
            req = self.with_auth(req, audit_reason)?;
            if let Some(b) = &body {
                req = req
                    .header(reqwest::header::CONTENT_TYPE, "application/json")
                    .body(b.clone());
            }
            let resp: reqwest::Response = req.send().await.map_err(|e| {
                if e.is_timeout() {
                    common::Error::Timeout(Box::from("request timeout"))
                } else {
                    common::Error::Network(Box::from("send failed"))
                }
            })?;
            let status = resp.status().as_u16();
            let headers = resp.headers().clone();
            self.record_headers(&bucket, &headers);
            let (_scope, global_hdr) = Self::scope_and_global(&headers);
            if status == 401 {
                return Err(common::Error::Unauthorized);
            }
            if status == 403 {
                return Err(common::Error::Forbidden);
            }
            if status == 429 {
                let bytes = resp.bytes().await.unwrap_or_default();
                #[derive(serde::Deserialize)]
                struct R {
                    #[serde(default)]
                    retry_after: f64,
                    #[serde(default)]
                    global: bool,
                }
                let p: R = serde_json::from_slice(&bytes).unwrap_or(R {
                    retry_after: 1.0,
                    global: global_hdr,
                });
                let global = p.global || global_hdr;
                self.ratelimiter_lock()
                    .retry_after_for_429(p.retry_after, global);
                tokio::time::sleep(Duration::from_secs_f64(p.retry_after.max(0.0))).await;
                if attempts >= 3 {
                    return Err(common::Error::RateLimited {
                        bucket: bucket.clone().into(),
                        retry_after_secs: p.retry_after,
                        global,
                    });
                }
                continue;
            }
            if status == 404 {
                return Err(common::Error::NotFound(Box::from("webhook")));
            }
            if (500..600).contains(&status) {
                if crate::send::should_retry(crate::Method::Post, status, attempts) {
                    tokio::time::sleep(Duration::from_millis(200 * u64::from(attempts))).await;
                    continue;
                }
                return Err(common::Error::Rest {
                    status,
                    code: 0,
                    message: Box::from("server error"),
                    errors: None,
                });
            }
            if !(200..300).contains(&status) {
                let bytes = resp.bytes().await.unwrap_or_default();
                #[derive(serde::Deserialize)]
                struct D {
                    #[serde(default)]
                    message: String,
                    #[serde(default)]
                    code: i64,
                }
                let p: D = serde_json::from_slice(&bytes).unwrap_or(D {
                    message: String::from("error"),
                    code: 0,
                });
                return Err(common::Error::Rest {
                    status,
                    code: p.code,
                    message: p.message.into(),
                    errors: None,
                });
            }
            let bytes = resp
                .bytes()
                .await
                .map_err(|_| common::Error::Network(Box::from("read failed")))?;
            return Ok(bytes.to_vec());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_redacts_token() {
        let c = Client::builder(secrecy::SecretString::from(String::from(
            "super-secret-token",
        )))
        .build()
        .unwrap();
        let d = format!("{c:?}");
        assert!(!d.contains("super-secret-token"));
        assert_eq!(c.auth()[..4], *"Bot ");
    }

    #[test]
    fn builder_debug_redacts_token() {
        let b = Client::builder(secrecy::SecretString::from(String::from(
            "builder-secret-token",
        )));
        let d = format!("{b:?}");
        assert!(
            !d.contains("builder-secret-token"),
            "ClientBuilder Debug must redact SecretString"
        );
    }

    #[test]
    fn request_ids_monotonic() {
        let a = common::next_request_id();
        let b = common::next_request_id();
        assert!(b > a, "request ids must increase: {a} -> {b}");
    }
}
