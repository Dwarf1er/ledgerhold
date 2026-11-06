//! ledgerhold-core: domain types and pure logic (no I/O).

pub mod dedup;
pub mod model;
pub mod money;

pub use model::*;
pub use money::{Money, MoneyError};
