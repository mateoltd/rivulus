//! Command registry with permission guards (no cooldowns/macros in v1).
use std::collections::HashMap;
use std::sync::Arc;

/// Command handler signature.
pub type Handler = Arc<dyn Fn(HandlerCtx) + Send + Sync + 'static>;

/// Context passed to handlers.
#[derive(Debug, Clone)]
pub struct HandlerCtx {
    /// Required permissions (None = no guard).
    pub required: Option<model::Permissions>,
    /// Caller permissions.
    pub caller: model::Permissions,
}

/// Registered command: `custom_id`s (components/modals) route by registering
/// the full id string as `name`; [`crate::split_command`] splits embedded
/// arguments when a payload carries them.
pub struct Command {
    /// Name (slash name or full `custom_id`).
    pub name: Box<str>,
    /// Permission guard (None = no guard).
    pub permission_guard: Option<model::Permissions>,
    /// Handler.
    pub handler: Handler,
}

impl Command {
    /// Required permissions alias for [`Command::permission_guard`].
    #[must_use]
    pub fn required(&self) -> Option<model::Permissions> {
        self.permission_guard
    }
}

impl std::fmt::Debug for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Command").field("name", &self.name).finish()
    }
}

/// Registry of name/`custom_id` -> [`Command`].
#[derive(Default)]
pub struct Registry {
    /// Registered commands by name.
    pub commands: HashMap<Box<str>, Command>,
}

impl Registry {
    /// New empty.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
    /// Register `name` with `handler` (no permission guard).
    pub fn register(&mut self, name: impl Into<Box<str>>, handler: Handler) {
        let name = name.into();
        self.commands.insert(
            name.clone(),
            Command {
                name,
                permission_guard: None,
                handler,
            },
        );
    }
    /// Register `name` with `handler` requiring `required` permissions.
    pub fn register_guarded(
        &mut self,
        name: impl Into<Box<str>>,
        handler: Handler,
        required: model::Permissions,
    ) {
        let name = name.into();
        self.commands.insert(
            name.clone(),
            Command {
                name,
                permission_guard: Some(required),
                handler,
            },
        );
    }
    /// Register a prebuilt [`Command`] (custom guard wiring).
    pub fn register_command(&mut self, cmd: Command) {
        self.commands.insert(cmd.name.clone(), cmd);
    }
    /// Look up a command.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Command> {
        self.commands.get(name)
    }
    /// Number of registered commands.
    #[must_use]
    pub fn len(&self) -> usize {
        self.commands.len()
    }
    /// True when no commands are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
    /// Dispatch with permission guard.
    ///
    /// # Errors
    /// Returns [`common::Error::NotFound`] for unknown names and
    /// [`common::Error::Forbidden`] when the caller lacks permissions.
    pub fn dispatch(&self, name: &str, caller: model::Permissions) -> Result<(), common::Error> {
        let Some(cmd) = self.commands.get(name) else {
            return Err(common::Error::NotFound(Box::from(name)));
        };
        if !check_permissions(caller, cmd.permission_guard) {
            return Err(common::Error::Forbidden);
        }
        (cmd.handler)(HandlerCtx {
            required: cmd.permission_guard,
            caller,
        });
        Ok(())
    }
}

impl std::fmt::Debug for Registry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Registry")
            .field("len", &self.commands.len())
            .finish()
    }
}

/// True when `have` satisfies `required` (None = no guard).
///
/// `ADMINISTRATOR` implies all: callers holding it pass any guard.
#[must_use]
pub fn check_permissions(have: model::Permissions, required: Option<model::Permissions>) -> bool {
    match required {
        None => true,
        Some(r) if r.is_empty() => true,
        Some(r) => have.contains(model::Permissions::ADMINISTRATOR) || have.contains(r),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noop() -> Handler {
        Arc::new(|_: HandlerCtx| {})
    }

    #[test]
    fn register_and_dispatch() {
        let mut reg = Registry::new();
        assert!(reg.is_empty());
        reg.register("ping", noop());
        assert_eq!(reg.len(), 1);
        assert!(reg.get("ping").is_some());
        assert!(reg.dispatch("ping", model::Permissions::empty()).is_ok());
        assert!(matches!(
            reg.dispatch("nope", model::Permissions::empty()),
            Err(common::Error::NotFound(_))
        ));
    }

    #[test]
    fn permission_guard() {
        let mut reg = Registry::new();
        reg.register_guarded("ban", noop(), model::Permissions::BAN_MEMBERS);
        let cmd = reg.get("ban").unwrap();
        assert_eq!(cmd.permission_guard, Some(model::Permissions::BAN_MEMBERS));
        // Without the bit: forbidden.
        assert!(matches!(
            reg.dispatch("ban", model::Permissions::SEND_MESSAGES),
            Err(common::Error::Forbidden)
        ));
        // With the bit: ok.
        assert!(reg.dispatch("ban", model::Permissions::BAN_MEMBERS).is_ok());
        // Administrator bypasses the guard.
        assert!(reg
            .dispatch("ban", model::Permissions::ADMINISTRATOR)
            .is_ok());
        // Free function mirrors dispatch.
        assert!(!check_permissions(
            model::Permissions::SEND_MESSAGES,
            Some(model::Permissions::BAN_MEMBERS)
        ));
        assert!(check_permissions(
            model::Permissions::BAN_MEMBERS,
            Some(model::Permissions::BAN_MEMBERS)
        ));
        assert!(check_permissions(model::Permissions::empty(), None));
    }
}
