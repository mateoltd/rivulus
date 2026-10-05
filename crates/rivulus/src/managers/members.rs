//! Member manager.
use std::sync::Arc;

/// Member operations.
///
/// Holds an owned [`crate::Context`] clone (rather than `&'a Context`) so
/// `stream_pages` can return `'static` streams; documented deviation from
/// the `X<'a>` sketch in plans/02 §4.
#[derive(Debug, Clone)]
pub struct Members {
    ctx: crate::Context,
}

impl Members {
    /// New bound to a context.
    #[must_use]
    pub fn new(ctx: crate::Context) -> Self {
        Self { ctx }
    }
    /// Sync cache lookup.
    #[must_use]
    pub fn get(&self, guild: model::GuildId, user: model::UserId) -> Option<Arc<model::Member>> {
        self.ctx.cache.get_member(guild, user)
    }
    /// Async fetch.
    ///
    /// Cache first, then `GET /guilds/{g}/members/{u}` via
    /// [`rest::Route::GetMember`].
    ///
    /// # Errors
    /// Returns [`common::Error`] when the request fails.
    pub async fn fetch(
        &self,
        guild: model::GuildId,
        user: model::UserId,
    ) -> Result<Arc<model::Member>, common::Error> {
        if let Some(m) = self.get(guild, user) {
            return Ok(m);
        }
        let route = rest::Route::GetMember {
            guild_id: guild.get(),
            user_id: user.get(),
        };
        let bytes = self.ctx.http.execute(&route, None, None).await?;
        let m: model::Member =
            serde_json::from_slice(&bytes).map_err(|e| common::Error::Deserialize {
                event: Box::from("GetMember"),
                reason: common::error::truncate_source(&e.to_string()),
            })?;
        let arc = Arc::new(m);
        self.ctx.cache.insert_member(arc.clone());
        Ok(arc)
    }
    /// Fetch one page of members (dual API with [`Self::stream_pages`]).
    ///
    /// Uses [`rest::Route::ListMembers`]; the frozen P1a variant carries no
    /// `limit`/`after` fields so [`rest::Route::query`] is `None` for it and
    /// `limit` is applied client-side via `take(limit)` (`0` maps to `1`).
    ///
    /// # Errors
    /// Returns [`common::Error`] when the request fails.
    pub async fn fetch_page(
        &self,
        guild: model::GuildId,
        limit: u8,
    ) -> Result<Vec<Arc<model::Member>>, common::Error> {
        let route = rest::Route::ListMembers {
            guild_id: guild.get(),
        };
        let bytes = self.ctx.http.execute(&route, None, None).await?;
        let list: Vec<model::Member> =
            serde_json::from_slice(&bytes).map_err(|e| common::Error::Deserialize {
                event: Box::from("ListMembers"),
                reason: common::error::truncate_source(&e.to_string()),
            })?;
        let cap = usize::from(limit).max(1);
        let mut out = Vec::new();
        for m in list.into_iter().take(cap) {
            let arc = Arc::new(m);
            self.ctx.cache.insert_member(arc.clone());
            out.push(arc);
        }
        Ok(out)
    }
    /// Stream pages (dual API with [`Self::fetch_page`]).
    ///
    /// Single-page unfold until `ListMembers` gains `after` cursor support;
    /// yields the one [`Self::fetch_page`] result then ends.
    pub fn stream_pages(
        &self,
        guild: model::GuildId,
        limit: u8,
    ) -> impl futures::Stream<Item = Result<Vec<Arc<model::Member>>, common::Error>> {
        let mgr = self.clone();
        futures::stream::unfold(Some(limit), move |state| {
            let mgr = mgr.clone();
            async move {
                match state {
                    Some(lim) => Some((mgr.fetch_page(guild, lim).await, None)),
                    None => None,
                }
            }
        })
    }
}
