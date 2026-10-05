//! Channel manager.
use std::sync::Arc;

/// Channel operations.
///
/// Holds an owned [`crate::Context`] clone (rather than `&'a Context`) so
/// `stream_pages` can return `'static` streams without lifetime plumbing;
/// documented deviation from the `X<'a>` sketch in plans/02 §4.
#[derive(Debug, Clone)]
pub struct Channels {
    ctx: crate::Context,
}

impl Channels {
    /// New bound to a context.
    #[must_use]
    pub fn new(ctx: crate::Context) -> Self {
        Self { ctx }
    }
    /// Sync cache lookup.
    #[must_use]
    pub fn get(&self, id: model::ChannelId) -> Option<Arc<model::Channel>> {
        self.ctx.cache.get_channel(id)
    }
    /// Async fetch.
    ///
    /// Cache first, then `GET /channels/{id}` via [`rest::Route::GetChannel`].
    ///
    /// # Errors
    /// Returns [`common::Error`] when the request fails.
    pub async fn fetch(&self, id: model::ChannelId) -> Result<Arc<model::Channel>, common::Error> {
        if let Some(c) = self.get(id) {
            return Ok(c);
        }
        let route = rest::Route::GetChannel {
            channel_id: id.get(),
        };
        let bytes = self.ctx.http.execute(&route, None, None).await?;
        let c: model::Channel =
            serde_json::from_slice(&bytes).map_err(|e| common::Error::Deserialize {
                event: Box::from("GetChannel"),
                reason: common::error::truncate_source(&e.to_string()),
            })?;
        let arc = Arc::new(c);
        self.ctx.cache.insert_channel(arc.clone());
        Ok(arc)
    }
    /// Fetch one page of guild channels (dual API with [`Self::stream_pages`]).
    ///
    /// Uses [`rest::Route::ListGuildChannels`] (`GET /guilds/{id}/channels`,
    /// which defines no query params, so `limit` is applied client-side via
    /// `take(limit)` with `0` mapping to `1`).
    ///
    /// # Errors
    /// Returns [`common::Error`] when the request fails.
    pub async fn fetch_page(
        &self,
        guild_id: model::GuildId,
        limit: u8,
    ) -> Result<Vec<Arc<model::Channel>>, common::Error> {
        let route = rest::Route::ListGuildChannels {
            guild_id: guild_id.get(),
        };
        let bytes = self.ctx.http.execute(&route, None, None).await?;
        let list: Vec<model::Channel> =
            common::json::from_slice(&bytes).map_err(|e| common::Error::Deserialize {
                event: Box::from("ListGuildChannels"),
                reason: common::error::truncate_source(&e.to_string()),
            })?;
        let cap = usize::from(limit).max(1);
        let mut out = Vec::new();
        for c in list.into_iter().take(cap) {
            let arc = Arc::new(c);
            self.ctx.cache.insert_channel(arc.clone());
            out.push(arc);
        }
        Ok(out)
    }
    /// Stream pages (dual API with [`Self::fetch_page`]).
    ///
    /// Single-page unfold (no cursor until the list route gains one);
    /// yields the one [`Self::fetch_page`] result then ends.
    pub fn stream_pages(
        &self,
        guild_id: model::GuildId,
        limit: u8,
    ) -> impl futures::Stream<Item = Result<Vec<Arc<model::Channel>>, common::Error>> {
        let mgr = self.clone();
        futures::stream::unfold(Some(limit), move |state| {
            let mgr = mgr.clone();
            async move {
                match state {
                    Some(lim) => Some((mgr.fetch_page(guild_id, lim).await, None)),
                    None => None,
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn list_body_parses_as_vec_offline() {
        // Mock REST returns `{}` (not an array), so `fetch_page` is
        // live-only; this pins the `Vec` parse half offline.
        let body = br#"[{"id":"9","type":0,"name":"general"}]"#;
        let list: Vec<model::Channel> = common::json::from_slice(body).expect("channel vec parses");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name.as_deref(), Some("general"));
    }
}
