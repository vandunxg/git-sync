//! App-owned IPC contract (contracts/ipc.v1.json + contracts/dtos.schema.json).
//!
//! `generated` holds the DTO structs and command table produced by
//! `contracts/codegen.py`; `schema` validates raw JSON against the committed schema
//! before any typed parse, because serde alone does not enforce `required` for
//! nullable fields, patterns, lengths or numeric bounds.

pub mod envelope;
pub mod generated;
pub mod schema;
pub mod secret;

pub use generated::*;

/// Looks up an allowlisted command by name. Forbidden or unknown names return `None`.
pub fn command_spec(name: &str) -> Option<&'static CommandSpec> {
    COMMANDS.iter().find(|spec| spec.name == name)
}
