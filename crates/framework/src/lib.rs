#![forbid(unsafe_code)]
#![allow(missing_docs)]
//! `framework` - optional command framework (parser + registry + guards only).

pub mod parser;
pub mod registry;

pub use parser::{parse_args, split_command, Args};
pub use registry::{check_permissions, Command, Handler, HandlerCtx, Registry};
