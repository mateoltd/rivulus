//! Message manager (send/edit/delete/pin/bulk).
use std::sync::Arc;

/// Message operations.
///
/// Holds an owned [`crate::Context`] clone (rather than `&'a Context`) so
/// `stream_pages` can return `'static` streams; documented deviation from
/// the `X<'a>` sketch in plans/02 §4.
#[derive(Debug, Clone)]
pub struct Messages {
    ctx: crate::Context,
}

impl Messages {
    /// New bound to a context.
    #[must_use]
    pub fn new(ctx: crate::Context) -> Self {
        Self { ctx }
    }
    /// Sync cache lookup.
    #[must_use]
    pub fn get(&self, id: model::MessageId) -> Option<Arc<model::Message>> {
        self.ctx.cache.get_message(id)
    }
    /// Fetch one message.
    ///
    /// Cache first, then `GET /channels/{c}/messages/{m}` via
    /// [`rest::Route::GetMessage`]. Gap: `get(id)` alone cannot build the
    /// route (it needs the channel), so `fetch` takes `(channel, id)`
    /// (documented; `Route` is frozen).
    ///
    /// # Errors
    /// Returns [`common::Error`] when the request fails.
    pub async fn fetch(
        &self,
        channel: model::ChannelId,
        id: model::MessageId,
    ) -> Result<Arc<model::Message>, common::Error> {
        if let Some(m) = self.get(id) {
            return Ok(m);
        }
        let route = rest::Route::GetMessage {
            channel_id: channel.get(),
            message_id: id.get(),
        };
        let bytes = self.ctx.http.execute(&route, None, None).await?;
        let m: model::Message =
            serde_json::from_slice(&bytes).map_err(|e| common::Error::Deserialize {
                event: Box::from("GetMessage"),
                reason: common::error::truncate_source(&e.to_string()),
            })?;
        let arc = Arc::new(m);
        self.ctx.cache.insert_message(arc.clone());
        Ok(arc)
    }
    /// Send a message.
    ///
    /// # Errors
    /// Returns [`common::Error`] on validation or request failure.
    pub async fn send(
        &self,
        channel: model::ChannelId,
        content: &str,
    ) -> Result<Arc<model::Message>, common::Error> {
        common::validate::content(content)?;
        let route = rest::Route::CreateMessage {
            channel_id: channel.get(),
        };
        let body = serde_json::json!({ "content": content });
        let bytes = self
            .ctx
            .http
            .execute(
                &route,
                Some(
                    common::json::to_vec(&body).map_err(|e| common::Error::Deserialize {
                        event: Box::from("CreateMessage"),
                        reason: common::error::truncate_source(&e.to_string()),
                    })?,
                ),
                None,
            )
            .await?;
        let m: model::Message =
            serde_json::from_slice(&bytes).map_err(|e| common::Error::Deserialize {
                event: Box::from("CreateMessage"),
                reason: common::error::truncate_source(&e.to_string()),
            })?;
        let arc = Arc::new(m);
        self.ctx.cache.insert_message(arc.clone());
        Ok(arc)
    }
    /// Fetch one page of channel history (dual API with [`Self::stream_pages`]).
    ///
    /// Uses [`rest::Route::ListMessages`] (`GET /channels/{id}/messages`
    /// with `?limit=`); `limit` `0` maps to `1`. Each message is cached.
    ///
    /// # Errors
    /// Returns [`common::Error`] when the request fails.
    pub async fn fetch_page(
        &self,
        channel: model::ChannelId,
        limit: u8,
    ) -> Result<Vec<Arc<model::Message>>, common::Error> {
        let route = rest::Route::ListMessages {
            channel_id: channel.get(),
            limit: u64::from(limit.max(1)),
            after: None,
        };
        let bytes = self.ctx.http.execute(&route, None, None).await?;
        let list: Vec<model::Message> =
            common::json::from_slice(&bytes).map_err(|e| common::Error::Deserialize {
                event: Box::from("ListMessages"),
                reason: common::error::truncate_source(&e.to_string()),
            })?;
        let mut out = Vec::new();
        for m in list {
            let arc = Arc::new(m);
            self.ctx.cache.insert_message(arc.clone());
            out.push(arc);
        }
        Ok(out)
    }
    /// Stream pages (dual API with [`Self::fetch_page`]).
    ///
    /// Single-page unfold until `ListMessages` lands; yields the one
    /// [`Self::fetch_page`] result then ends.
    pub fn stream_pages(
        &self,
        channel: model::ChannelId,
        limit: u8,
    ) -> impl futures::Stream<Item = Result<Vec<Arc<model::Message>>, common::Error>> {
        let mgr = self.clone();
        futures::stream::unfold(Some(limit), move |state| {
            let mgr = mgr.clone();
            async move {
                match state {
                    Some(lim) => Some((mgr.fetch_page(channel, lim).await, None)),
                    None => None,
                }
            }
        })
    }
    /// Bulk delete messages (validates `2..=100` before any IO).
    ///
    /// Validates via [`common::validate::bulk_count`] first, so an invalid
    /// count fails fast without building a request.
    ///
    /// # Errors
    /// Returns [`common::Error::Validation`] when `message_ids.len()` is
    /// outside `2..=100`, else any REST error.
    pub async fn bulk_delete(
        &self,
        channel: model::ChannelId,
        message_ids: &[model::MessageId],
    ) -> Result<(), common::Error> {
        common::validate::bulk_count(message_ids.len())?;
        let route = rest::Route::BulkDelete {
            channel_id: channel.get(),
        };
        let ids: Vec<String> = message_ids.iter().map(|id| id.to_string()).collect();
        let body = serde_json::json!({ "messages": ids });
        let bytes = common::json::to_vec(&body).map_err(|e| common::Error::Deserialize {
            event: Box::from("BulkDelete"),
            reason: common::error::truncate_source(&e.to_string()),
        })?;
        self.ctx.http.execute(&route, Some(bytes), None).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_ctx() -> crate::Context {
        let http = Arc::new(
            rest::Client::builder(secrecy::SecretString::from(String::from("t")))
                .build()
                .expect("rest build"),
        );
        let cache: Arc<dyn cache::Cache> =
            Arc::new(cache::InMemoryCache::new(cache::CacheConfig::minimal()));
        #[cfg(feature = "standby")]
        let standby = Arc::new(standby::Standby::new());
        crate::Context {
            http,
            cache,
            #[cfg(feature = "standby")]
            standby,
            shard: gateway::ShardMessenger {
                shard: gateway::ShardId { id: 0, total: 1 },
                latency: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            },
        }
    }

    #[test]
    fn list_body_parses_as_vec_offline() {
        // The mock REST server returns `{}` (not an array), so `fetch_page`
        // itself is live-only; this pins the `Vec` parse half offline using
        // the same `common::json::from_slice` call the manager makes.
        let body = br#"[{"id":"4","channel_id":"3","author":{"id":"2","username":"u"},"content":"hi","timestamp":"2024-01-01T00:00:00Z"},{"id":"5","channel_id":"3","author":{"id":"2","username":"u"},"content":"yo","timestamp":"2024-01-01T00:00:00Z"}]"#;
        let list: Vec<model::Message> = common::json::from_slice(body).expect("message vec parses");
        assert_eq!(list.len(), 2);
        assert_eq!(&list[0].content as &str, "hi");
        // `minimal()` disables message storage, so insert into a
        // message-enabled cache to prove the parsed ids round-trip.
        let full = cache::CacheConfig {
            message_limit: 10,
            resource_types: cache::ResourceType::MESSAGE,
            ..cache::CacheConfig::minimal()
        };
        let cache: Arc<dyn cache::Cache> = Arc::new(cache::InMemoryCache::new(full));
        for m in list {
            cache.insert_message(Arc::new(m));
        }
        let id = model::MessageId::new(4).expect("id");
        assert!(cache.get_message(id).is_some());
    }

    #[tokio::test]
    async fn bulk_delete_validates_before_io() {
        let mgr = Messages::new(test_ctx());
        let channel = model::ChannelId::new(1).expect("channel");
        // 0 and 1 fail fast with Validation (no IO attempted).
        assert!(mgr.bulk_delete(channel, &[]).await.is_err());
        let one = [model::MessageId::new(2).expect("id")];
        let err = mgr.bulk_delete(channel, &one).await.unwrap_err();
        assert!(matches!(err, common::Error::Validation(_)));
        // 101 also fails fast.
        let many: Vec<model::MessageId> = (1..=101u64)
            .map(|n| model::MessageId::new(n).expect("id"))
            .collect();
        let err = mgr.bulk_delete(channel, &many).await.unwrap_err();
        assert!(matches!(err, common::Error::Validation(_)));
        // Direct validator parity.
        assert!(common::validate::bulk_count(2).is_ok());
        assert!(common::validate::bulk_count(100).is_ok());
        assert!(common::validate::bulk_count(1).is_err());
    }
}
