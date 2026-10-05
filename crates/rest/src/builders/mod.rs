//! Owned request builders (sync, no IO; all validate before serialize).
//!
//! Builders mirror discord.js `Builders`: [`CreateMessage`],
//! [`CreateEmbed`], [`ExecuteWebhook`], [`InteractionCallback`].

pub mod embed;
pub mod interaction;
pub mod message;
pub mod webhook;

pub use embed::CreateEmbed;
pub use interaction::InteractionCallback;
pub use message::CreateMessage;
pub use webhook::ExecuteWebhook;
