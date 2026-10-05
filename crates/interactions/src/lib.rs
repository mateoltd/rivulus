//! `interactions` - command registration, ed25519 verify, responses.
#![forbid(unsafe_code)]
#![allow(missing_docs)]

pub mod commands;
pub mod respond;
pub mod verify;

pub use commands::{bulk_overwrite_payload, diff_commands, diff_log, CommandDef, CommandDiff};
pub use respond::{
    ack_deadline_ok, followup_url, respond, AutocompleteChoice, CallbackBody, CallbackData,
    Response, ResponseKind,
};
pub use verify::{verify, verify_signature, MAX_BODY_BYTES};
