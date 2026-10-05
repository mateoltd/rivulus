//! Typed routes: method + path template + major id (bucket key never ad-hoc).
use crate::Method;

/// API version.
pub const API_VERSION: u8 = 10;
/// API base.
pub const BASE: &str = "https://discord.com/api/v10";

/// Typed route.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum Route {
    /// Get channel.
    GetChannel {
        /// Channel.
        channel_id: u64,
    },
    /// Create message.
    CreateMessage {
        /// Channel.
        channel_id: u64,
    },
    /// Get message.
    GetMessage {
        /// Channel.
        channel_id: u64,
        /// Message.
        message_id: u64,
    },
    /// Edit message.
    EditMessage {
        /// Channel.
        channel_id: u64,
        /// Message.
        message_id: u64,
    },
    /// Delete message.
    DeleteMessage {
        /// Channel.
        channel_id: u64,
        /// Message.
        message_id: u64,
    },
    /// Bulk delete.
    BulkDelete {
        /// Channel.
        channel_id: u64,
    },
    /// Get guild.
    GetGuild {
        /// Guild.
        guild_id: u64,
    },
    /// Create guild channel.
    CreateGuildChannel {
        /// Guild.
        guild_id: u64,
    },
    /// List members.
    ListMembers {
        /// Guild.
        guild_id: u64,
    },
    /// List guild channels (`GET /guilds/{id}/channels`; Discord defines no
    /// query params, so `query()` is `None` and `limit` is applied
    /// client-side by the manager).
    ListGuildChannels {
        /// Guild.
        guild_id: u64,
    },
    /// List channel messages (`GET /channels/{id}/messages`; `limit 1..=100`).
    ListMessages {
        /// Channel.
        channel_id: u64,
        /// Page size.
        limit: u64,
        /// Return messages after this id (cursor; `None` for the first page).
        after: Option<u64>,
    },
    /// List current-user guilds (`GET /users/@me/guilds`; `limit 1..=200`).
    ListCurrentUserGuilds {
        /// Page size.
        limit: u64,
    },
    /// Get member.
    GetMember {
        /// Guild.
        guild_id: u64,
        /// User.
        user_id: u64,
    },
    /// Create role.
    CreateRole {
        /// Guild.
        guild_id: u64,
    },
    /// Get audit log.
    GetAuditLog {
        /// Guild.
        guild_id: u64,
    },
    /// Create webhook.
    CreateWebhook {
        /// Channel.
        channel_id: u64,
    },
    /// Execute webhook (token is secret; only presence flag stored).
    ExecuteWebhook {
        /// Webhook.
        webhook_id: u64,
        /// Whether token present (never the token).
        token_is_present: bool,
    },
    /// Get gateway bot.
    GetGatewayBot,
    /// Create application command.
    CreateApplicationCommand {
        /// App.
        application_id: u64,
    },
    /// Create guild command.
    CreateGuildCommand {
        /// App.
        application_id: u64,
        /// Guild.
        guild_id: u64,
    },
    /// Interaction callback (token secret; presence only).
    InteractionCallback {
        /// Interaction.
        interaction_id: u64,
        /// Whether token present.
        token_is_present: bool,
    },
}

impl Route {
    /// HTTP method.
    #[must_use]
    pub fn method(&self) -> Method {
        match self {
            Self::GetChannel { .. }
            | Self::GetMessage { .. }
            | Self::GetGuild { .. }
            | Self::ListMembers { .. }
            | Self::ListGuildChannels { .. }
            | Self::ListMessages { .. }
            | Self::ListCurrentUserGuilds { .. }
            | Self::GetMember { .. }
            | Self::GetAuditLog { .. }
            | Self::GetGatewayBot => Method::Get,
            Self::CreateMessage { .. }
            | Self::BulkDelete { .. }
            | Self::CreateGuildChannel { .. }
            | Self::CreateRole { .. }
            | Self::CreateWebhook { .. }
            | Self::ExecuteWebhook { .. }
            | Self::CreateApplicationCommand { .. }
            | Self::CreateGuildCommand { .. }
            | Self::InteractionCallback { .. } => Method::Post,
            Self::EditMessage { .. } => Method::Patch,
            Self::DeleteMessage { .. } => Method::Delete,
        }
    }
    /// Path template (low-cardinality metric label).
    #[must_use]
    pub fn path_template(&self) -> &'static str {
        match self {
            Self::GetChannel { .. } => "/channels/{channel_id}",
            Self::CreateMessage { .. } | Self::ListMessages { .. } => {
                "/channels/{channel_id}/messages"
            }
            Self::GetMessage { .. } | Self::EditMessage { .. } | Self::DeleteMessage { .. } => {
                "/channels/{channel_id}/messages/{message_id}"
            }
            Self::BulkDelete { .. } => "/channels/{channel_id}/messages/bulk-delete",
            Self::GetGuild { .. } => "/guilds/{guild_id}",
            Self::CreateGuildChannel { .. } | Self::ListGuildChannels { .. } => {
                "/guilds/{guild_id}/channels"
            }
            Self::ListMembers { .. } => "/guilds/{guild_id}/members",
            Self::GetMember { .. } => "/guilds/{guild_id}/members/{user_id}",
            Self::CreateRole { .. } => "/guilds/{guild_id}/roles",
            Self::GetAuditLog { .. } => "/guilds/{guild_id}/audit-logs",
            Self::CreateWebhook { .. } => "/channels/{channel_id}/webhooks",
            Self::ListCurrentUserGuilds { .. } => "/users/@me/guilds",
            Self::ExecuteWebhook { .. } => "/webhooks/{webhook_id}/{token}",
            Self::GetGatewayBot => "/gateway/bot",
            Self::CreateApplicationCommand { .. } => "/applications/{application_id}/commands",
            Self::CreateGuildCommand { .. } => {
                "/applications/{application_id}/guilds/{guild_id}/commands"
            }
            Self::InteractionCallback { .. } => "/interactions/{interaction_id}/{token}/callback",
        }
    }
    /// Major parameter (channel/guild/webhook id) for bucket isolation.
    #[must_use]
    pub fn major_id(&self) -> Option<u64> {
        match *self {
            Self::GetChannel { channel_id }
            | Self::CreateMessage { channel_id }
            | Self::ListMessages { channel_id, .. }
            | Self::BulkDelete { channel_id }
            | Self::CreateWebhook { channel_id } => Some(channel_id),
            Self::GetMessage { channel_id, .. }
            | Self::EditMessage { channel_id, .. }
            | Self::DeleteMessage { channel_id, .. } => Some(channel_id),
            Self::GetGuild { guild_id }
            | Self::CreateGuildChannel { guild_id }
            | Self::ListGuildChannels { guild_id }
            | Self::ListMembers { guild_id }
            | Self::GetMember { guild_id, .. }
            | Self::CreateRole { guild_id }
            | Self::GetAuditLog { guild_id } => Some(guild_id),
            Self::CreateGuildCommand { guild_id, .. } => Some(guild_id),
            Self::CreateApplicationCommand { application_id } => Some(application_id),
            Self::ExecuteWebhook { webhook_id, .. } => Some(webhook_id),
            Self::InteractionCallback { interaction_id, .. } => Some(interaction_id),
            Self::ListCurrentUserGuilds { .. } | Self::GetGatewayBot => None,
        }
    }
    /// Concrete path with ids.
    #[must_use]
    pub fn path(&self) -> String {
        fn id(n: u64) -> String {
            let mut b = itoa::Buffer::new();
            b.format(n).to_owned()
        }
        match *self {
            Self::GetChannel { channel_id } => format!("/channels/{}", id(channel_id)),
            Self::CreateMessage { channel_id } => format!("/channels/{}/messages", id(channel_id)),
            Self::ListMessages { channel_id, .. } => {
                format!("/channels/{}/messages", id(channel_id))
            }
            Self::GetMessage {
                channel_id,
                message_id,
            }
            | Self::EditMessage {
                channel_id,
                message_id,
            }
            | Self::DeleteMessage {
                channel_id,
                message_id,
            } => {
                format!("/channels/{}/messages/{}", id(channel_id), id(message_id))
            }
            Self::BulkDelete { channel_id } => {
                format!("/channels/{}/messages/bulk-delete", id(channel_id))
            }
            Self::GetGuild { guild_id } => format!("/guilds/{}", id(guild_id)),
            Self::CreateGuildChannel { guild_id } => format!("/guilds/{}/channels", id(guild_id)),
            Self::ListGuildChannels { guild_id } => format!("/guilds/{}/channels", id(guild_id)),
            Self::ListMembers { guild_id } => format!("/guilds/{}/members", id(guild_id)),
            Self::GetMember { guild_id, user_id } => {
                format!("/guilds/{}/members/{}", id(guild_id), id(user_id))
            }
            Self::CreateRole { guild_id } => format!("/guilds/{}/roles", id(guild_id)),
            Self::GetAuditLog { guild_id } => format!("/guilds/{}/audit-logs", id(guild_id)),
            Self::CreateWebhook { channel_id } => format!("/channels/{}/webhooks", id(channel_id)),
            Self::ListCurrentUserGuilds { .. } => String::from("/users/@me/guilds"),
            Self::ExecuteWebhook { webhook_id, .. } => format!("/webhooks/{}", id(webhook_id)),
            Self::GetGatewayBot => String::from("/gateway/bot"),
            Self::CreateApplicationCommand { application_id } => {
                format!("/applications/{}/commands", id(application_id))
            }
            Self::CreateGuildCommand {
                application_id,
                guild_id,
            } => {
                format!(
                    "/applications/{}/guilds/{}/commands",
                    id(application_id),
                    id(guild_id)
                )
            }
            Self::InteractionCallback { interaction_id, .. } => {
                format!("/interactions/{}/callback", id(interaction_id))
            }
        }
    }
    /// Query string without the leading `?` (`None` when the route takes
    /// no query params). Only list variants that Discord defines params
    /// for return `Some`; all other routes return `None`.
    #[must_use]
    pub fn query(&self) -> Option<String> {
        match *self {
            Self::ListMessages { limit, after, .. } => {
                let mut q = format!("limit={}", limit.max(1));
                if let Some(a) = after {
                    q.push_str("&after=");
                    let mut b = itoa::Buffer::new();
                    q.push_str(b.format(a));
                }
                Some(q)
            }
            Self::ListCurrentUserGuilds { limit } => Some(format!("limit={}", limit.max(1))),
            _ => None,
        }
    }
    /// Full URL.
    #[must_use]
    pub fn url(&self) -> String {
        self.url_with_base(BASE)
    }
    /// Full URL against a custom base (tests point at a mock server).
    #[must_use]
    pub fn url_with_base(&self, base: &str) -> String {
        let path = self.path();
        match self.query() {
            Some(q) => format!("{}{path}?{q}", base.trim_end_matches('/')),
            None => format!("{}{path}", base.trim_end_matches('/')),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_messages_query() {
        let r = Route::ListMessages {
            channel_id: 3,
            limit: 50,
            after: None,
        };
        assert_eq!(r.query(), Some(String::from("limit=50")));
        let r = Route::ListMessages {
            channel_id: 3,
            limit: 25,
            after: Some(7),
        };
        assert_eq!(r.query(), Some(String::from("limit=25&after=7")));
        let r = Route::ListMessages {
            channel_id: 3,
            limit: 0,
            after: None,
        };
        assert_eq!(r.query(), Some(String::from("limit=1")));
    }

    #[test]
    fn list_guilds_query_and_plain_routes_none() {
        let r = Route::ListCurrentUserGuilds { limit: 100 };
        assert_eq!(r.query(), Some(String::from("limit=100")));
        assert_eq!(Route::ListGuildChannels { guild_id: 9 }.query(), None);
        assert_eq!(Route::ListMembers { guild_id: 9 }.query(), None);
        assert_eq!(Route::GetGatewayBot.query(), None);
        assert_eq!(Route::GetChannel { channel_id: 1 }.query(), None);
    }

    #[test]
    fn list_url_shapes() {
        let base = "http://127.0.0.1:9";
        let r = Route::ListMessages {
            channel_id: 3,
            limit: 50,
            after: None,
        };
        assert_eq!(
            r.url_with_base(base),
            "http://127.0.0.1:9/channels/3/messages?limit=50"
        );
        let r = Route::ListCurrentUserGuilds { limit: 10 };
        assert_eq!(
            r.url_with_base(base),
            "http://127.0.0.1:9/users/@me/guilds?limit=10"
        );
        let r = Route::ListGuildChannels { guild_id: 9 };
        assert_eq!(
            r.url_with_base(base),
            "http://127.0.0.1:9/guilds/9/channels"
        );
        assert_eq!(Route::GetGatewayBot.url(), format!("{BASE}/gateway/bot"));
    }

    #[test]
    fn list_bucket_key_isolation() {
        let get = Route::ListMessages {
            channel_id: 3,
            limit: 50,
            after: None,
        };
        let post = Route::CreateMessage { channel_id: 3 };
        assert_ne!(
            crate::Ratelimiter::bucket_key(&get),
            crate::Ratelimiter::bucket_key(&post)
        );
        let a = Route::ListGuildChannels { guild_id: 1 };
        let b = Route::ListGuildChannels { guild_id: 2 };
        assert_ne!(
            crate::Ratelimiter::bucket_key(&a),
            crate::Ratelimiter::bucket_key(&b)
        );
        let a = Route::ListMessages {
            channel_id: 1,
            limit: 10,
            after: None,
        };
        let b = Route::ListMessages {
            channel_id: 2,
            limit: 10,
            after: None,
        };
        assert_ne!(
            crate::Ratelimiter::bucket_key(&a),
            crate::Ratelimiter::bucket_key(&b)
        );
        assert_eq!(
            crate::Ratelimiter::bucket_key(&Route::ListCurrentUserGuilds { limit: 10 }),
            "GET:/users/@me/guilds"
        );
        assert_eq!(get.method(), crate::Method::Get);
        assert_eq!(get.major_id(), Some(3));
        assert_eq!(Route::ListCurrentUserGuilds { limit: 10 }.major_id(), None);
    }
}
