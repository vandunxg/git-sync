//! Infrastructure adapters. Each module implements narrow application ports.

pub mod filesystem;
pub mod git;
pub mod git_auth;
pub mod github;
pub mod gitlab;
mod http;
pub mod platform;
pub mod secrets;
pub mod snapshots;
pub mod sqlite;
pub mod storage;
pub mod workspace;
