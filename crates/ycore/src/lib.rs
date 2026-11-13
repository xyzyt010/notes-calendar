//! ycore — shared core for the yellod notes+calendar apps.
//!
//! Everything here is UI-agnostic and fully on-device:
//! - data model (notes, folders, events, reminders)
//! - SQLite storage with FTS5 full-text search
//! - small utilities shared by the native and web frontends

pub mod crypto;
pub mod model;
pub mod storage;
pub mod sys;

pub use model::*;
