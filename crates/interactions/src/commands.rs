//! Command registration (bulk-overwrite payload + diff log).
use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

/// Application command definition (bulk-overwrite PUT body item).
///
/// `kind`: `1` = chat-input (slash), `2` = user, `3` = message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandDef {
    /// Command name (validated lowercase, `<= 32`).
    pub name: Box<str>,
    /// Command description (`<= 100` chars; required for slash commands).
    pub description: Box<str>,
    /// Command kind: `1` slash, `2` user, `3` message.
    #[serde(rename = "type")]
    pub kind: u8,
    /// Command options, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<model::CommandOption>>,
}

impl CommandDef {
    /// New slash-command definition (`kind = 1`).
    #[must_use]
    pub fn slash(name: &str, description: &str) -> Self {
        Self {
            name: Box::from(name),
            description: Box::from(description),
            kind: 1,
            options: None,
        }
    }

    /// Validate before registration (names via [`common::validate::command_name`]).
    ///
    /// # Errors
    /// Returns [`common::Error::Validation`] on a bad name, unknown `kind`
    /// (must be `1`–`3`), a bad description, or a bad option.
    pub fn validate(&self) -> Result<(), common::Error> {
        common::validate::command_name(&self.name)?;
        match self.kind {
            1 => {
                if self.description.is_empty() || self.description.chars().count() > 100 {
                    return Err(common::Error::Validation(Box::from(
                        "slash command description must be 1-100 chars",
                    )));
                }
            }
            2 | 3 => {
                if self.description.chars().count() > 100 {
                    return Err(common::Error::Validation(Box::from(
                        "command description must be <= 100 chars",
                    )));
                }
            }
            _ => {
                return Err(common::Error::Validation(Box::from(
                    "command kind must be 1-3",
                )));
            }
        }
        if let Some(opts) = &self.options {
            for o in opts {
                common::validate::command_name(&o.name)?;
                if !(1..=11).contains(&o.kind) {
                    return Err(common::Error::Validation(Box::from(
                        "command option kind must be 1-11",
                    )));
                }
            }
        }
        Ok(())
    }
}

/// Serialize bulk-overwrite PUT body for `defs` (validates nothing; call
/// [`CommandDef::validate`] first). Serialization of owned defs cannot fail;
/// a serialization error yields an empty body rather than panicking.
#[must_use]
pub fn bulk_overwrite_payload(defs: &[CommandDef]) -> Vec<u8> {
    common::json::to_vec(defs).unwrap_or_default()
}

/// Human-readable diff of desired (`new`) vs remote (`old`) command sets.
///
/// Compares by name: names only in `new` are `added`, only in `old` are
/// `removed`, and names in both with differing defs are `changed`.
/// Returns `"no changes"` when the sets match.
#[must_use]
pub fn diff_log(old: &[CommandDef], new: &[CommandDef]) -> String {
    let o: HashMap<&str, &CommandDef> = old.iter().map(|c| (c.name.as_ref(), c)).collect();
    let n: HashMap<&str, &CommandDef> = new.iter().map(|c| (c.name.as_ref(), c)).collect();
    let mut added: Vec<&str> = Vec::new();
    let mut removed: Vec<&str> = Vec::new();
    let mut changed: Vec<&str> = Vec::new();
    for (name, def) in &n {
        match o.get(name) {
            None => added.push(name),
            Some(prev) => {
                if *prev != *def {
                    changed.push(name);
                }
            }
        }
    }
    for name in o.keys() {
        if !n.contains_key(name) {
            removed.push(name);
        }
    }
    if added.is_empty() && removed.is_empty() && changed.is_empty() {
        return String::from("no changes");
    }
    added.sort_unstable();
    removed.sort_unstable();
    changed.sort_unstable();
    let mut out = String::new();
    if !added.is_empty() {
        out.push_str("added: ");
        out.push_str(&added.join(", "));
    }
    if !removed.is_empty() {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str("removed: ");
        out.push_str(&removed.join(", "));
    }
    if !changed.is_empty() {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str("changed: ");
        out.push_str(&changed.join(", "));
    }
    out
}

/// Command identity for diffing.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CommandDiff {
    /// Command name.
    pub name: Box<str>,
    /// Changed (vs only added or removed).
    pub changed: bool,
}

/// Diff desired vs remote command names (logs before bulk PUT).
#[must_use]
pub fn diff_commands(desired: &[Box<str>], remote: &[Box<str>]) -> Vec<CommandDiff> {
    let r: HashSet<&str> = remote.iter().map(AsRef::as_ref).collect();
    let d: HashMap<&str, ()> = desired.iter().map(|s| (s.as_ref(), ())).collect();
    let mut out: Vec<CommandDiff> = desired
        .iter()
        .map(|n| CommandDiff {
            name: n.clone(),
            changed: !r.contains(n.as_ref()),
        })
        .collect();
    for name in remote {
        if !d.contains_key(name.as_ref()) {
            out.push(CommandDiff {
                name: name.clone(),
                changed: true,
            });
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(name: &str, description: &str) -> CommandDef {
        CommandDef::slash(name, description)
    }

    #[test]
    fn validate_names() {
        assert!(def("ping", "Ping it").validate().is_ok());
        // Uppercase rejected.
        assert!(def("Ping", "Ping it").validate().is_err());
        // Spaces rejected.
        assert!(def("my cmd", "Ping it").validate().is_err());
        // Empty rejected.
        assert!(def("", "Ping it").validate().is_err());
        // Over 32 chars rejected.
        assert!(def(&"a".repeat(33), "Ping it").validate().is_err());
        // Exactly 32 ok.
        assert!(def(&"a".repeat(32), "Ping it").validate().is_ok());
        // Empty description rejected for slash.
        assert!(def("ping", "").validate().is_err());
        // Unknown kind rejected.
        let mut bad = def("ping", "Ping it");
        bad.kind = 4;
        assert!(bad.validate().is_err());
    }

    #[test]
    fn payload_roundtrip() {
        let defs = [def("ping", "Ping it"), def("pong", "Pong it")];
        for d in &defs {
            assert!(d.validate().is_ok());
        }
        let body = bulk_overwrite_payload(&defs);
        assert!(!body.is_empty());
        let back: Vec<CommandDef> = common::json::from_slice(&body).unwrap_or_default();
        assert_eq!(back, defs);
        assert!(bulk_overwrite_payload(&[]).starts_with(b"["));
    }

    #[test]
    fn diff_log_groups() {
        let old = [
            def("ping", "Ping it"),
            def("gone", "Old"),
            def("tweak", "v1"),
        ];
        let mut tweaked = def("tweak", "v2");
        tweaked.kind = 1;
        let new = [def("ping", "Ping it"), def("fresh", "New"), tweaked];
        let log = diff_log(&old, &new);
        assert!(log.contains("added: fresh"), "{log}");
        assert!(log.contains("removed: gone"), "{log}");
        assert!(log.contains("changed: tweak"), "{log}");
        assert_eq!(diff_log(&old, &old), "no changes");
        assert_eq!(diff_log(&[], &[]), "no changes");
    }

    #[test]
    fn v1_v2_split_via_rest_builders() {
        fn row() -> model::Component {
            model::Component {
                kind: model::ComponentKind::ActionRow,
                custom_id: None,
                label: None,
                style: None,
                components: Vec::new(),
                content: None,
            }
        }
        // V1: 6 rows rejected.
        let mut v1 = rest::InteractionCallback::new(4).content("hi");
        for _ in 0..6 {
            v1 = v1.component(row());
        }
        assert!(v1.validate().is_err());
        // V2: 6 top-level accepted.
        let mut v2 = rest::InteractionCallback::new(4)
            .content("hi")
            .components_v2(true);
        for _ in 0..6 {
            v2 = v2.component(row());
        }
        assert!(v2.validate().is_ok());
        // V2 top cap (40) enforced.
        let mut huge = rest::InteractionCallback::new(4).components_v2(true);
        for _ in 0..41 {
            huge = huge.component(row());
        }
        assert!(huge.validate().is_err());
        // Unknown callback kind (incl. 11) rejected.
        assert!(rest::InteractionCallback::new(11).validate().is_err());
        assert!(rest::InteractionCallback::new(3).validate().is_err());
    }
}
