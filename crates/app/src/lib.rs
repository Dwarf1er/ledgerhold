//! Application service layer shared by the CLI and the desktop app: DTOs that
//! serialize cleanly to JSON (camelCase, minor-unit money, ISO dates) and the
//! operations a UI needs, all testable without a window.

pub mod dto;
pub mod helpers;
pub mod rpc;
mod service;

pub use helpers::{db_path_for, default_db_path, kebab, month_bounds, parse_month, select_statement};
pub use service::*;
