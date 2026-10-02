//! Infrastructure adapters. Each module implements narrow application ports.

pub mod git_auth;
pub mod github;
pub mod gitlab;
mod http;
pub mod platform;
pub mod secrets;
pub mod sqlite;
pub mod storage;
