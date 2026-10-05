//! Resource managers (thin fns over REST + cache; sync get + async fetch).
pub mod channels;
pub mod guilds;
pub mod members;
pub mod messages;

pub use channels::Channels;
pub use guilds::Guilds;
pub use members::Members;
pub use messages::Messages;

impl crate::Context {
    /// Channel manager bound to this context.
    #[must_use]
    pub fn channels(&self) -> Channels {
        Channels::new(self.clone())
    }
    /// Guild manager bound to this context.
    #[must_use]
    pub fn guilds(&self) -> Guilds {
        Guilds::new(self.clone())
    }
    /// Member manager bound to this context.
    #[must_use]
    pub fn members(&self) -> Members {
        Members::new(self.clone())
    }
    /// Message manager bound to this context.
    #[must_use]
    pub fn messages(&self) -> Messages {
        Messages::new(self.clone())
    }
}
