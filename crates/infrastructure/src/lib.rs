//! Infrastructure adapters. Each module implements narrow application ports.

pub mod github;
pub mod gitlab;
mod http;
pub mod secrets;
pub mod sqlite;
