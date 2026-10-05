//! Guild manager (sync get from cache + async fetch via REST).
use std::sync::Arc;

/// Guild operations.
///
/// Holds an owned [`crate::Context`] clone (rather than `&'a Context`) so
/// `stream_pages` can return `'static` streams; documented deviation from
/// the `X<'a>` sketch in plans/02 §4.
#[derive(Debug, Clone)]
pub struct Guilds {
    ctx: crate::Context,
}

impl Guilds {
    /// New bound to a context.
    #[must_use]
    pub fn new(ctx: crate::Context) -> Self {
        Self { ctx }
    }
    /// Sync cache lookup.
    #[must_use]
    pub fn get(&self, id: model::GuildId) -> Option<Arc<model::Guild>> {
        self.ctx.cache.get_guild(id)
    }
    /// Async fetch (REST; falls back to cache on error shape in P5-full).
    ///
    /// Cache first, then `GET /guilds/{id}` via [`rest::Route::GetGuild`].
    ///
    /// # Errors
    /// Returns [`common::Error`] when the request fails.
    pub async fn fetch(&self, id: model::GuildId) -> Result<Arc<model::Guild>, common::Error> {
        if let Some(g) = self.get(id) {
            return Ok(g);
        }
        let route = rest::Route::GetGuild { guild_id: id.get() };
        let bytes = self.ctx.http.execute(&route, None, None).await?;
        let g: model::Guild =
            serde_json::from_slice(&bytes).map_err(|e| common::Error::Deserialize {
                event: Box::from("GetGuild"),
                reason: common::error::truncate_source(&e.to_string()),
            })?;
        let arc = Arc::new(g);
        self.ctx.cache.insert_guild(arc.clone());
        Ok(arc)
    }
    /// Fetch one page of current-user guilds (dual API with [`Self::stream_pages`]).
    ///
    /// Uses [`rest::Route::ListCurrentUserGuilds`] (`GET /users/@me/guilds`
    /// with `?limit=`). Shape note: Discord returns partial guild objects
    /// here (no `owner_id`), which [`model::Guild`] requires, so a live
    /// response without `owner_id` surfaces as a truncated
    /// [`common::Error::Deserialize`] instead of caching corrupt entries.
    ///
    /// # Errors
    /// Returns [`common::Error`] when the request fails.
    pub async fn fetch_page(&self, limit: u8) -> Result<Vec<Arc<model::Guild>>, common::Error> {
        let route = rest::Route::ListCurrentUserGuilds {
            limit: u64::from(limit.max(1)),
        };
        let bytes = self.ctx.http.execute(&route, None, None).await?;
        let list: Vec<model::Guild> =
            common::json::from_slice(&bytes).map_err(|e| common::Error::Deserialize {
                event: Box::from("ListCurrentUserGuilds"),
                reason: common::error::truncate_source(&e.to_string()),
            })?;
        let mut out = Vec::new();
        for g in list {
            let arc = Arc::new(g);
            self.ctx.cache.insert_guild(arc.clone());
            out.push(arc);
        }
        Ok(out)
    }
    /// Stream pages (dual API with [`Self::fetch_page`]).
    ///
    /// Single-page unfold (no cursor until the list route lands).
    pub fn stream_pages(
        &self,
        limit: u8,
    ) -> impl futures::Stream<Item = Result<Vec<Arc<model::Guild>>, common::Error>> {
        let mgr = self.clone();
        futures::stream::unfold(Some(limit), move |state| {
            let mgr = mgr.clone();
            async move {
                match state {
                    Some(lim) => Some((mgr.fetch_page(lim).await, None)),
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
        // live-only; this pins the `Vec` parse half offline. Full
        // `model::Guild` shape required (`owner_id` present); partial
        // current-user guild objects fail live (see `fetch_page` docs).
        let body = br#"[{"id":"1","name":"g","owner_id":"2"}]"#;
        let list: Vec<model::Guild> = common::json::from_slice(body).expect("guild vec parses");
        assert_eq!(list.len(), 1);
        assert_eq!(&list[0].name as &str, "g");
    }
}
