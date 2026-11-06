//! OFX 1.02 (SGML) writer and a tolerant OFX 1.x/2.x reader.

mod reader;
mod writer;

pub use reader::{ReadError, read};
pub use writer::{OfxAccount, OfxExport, OfxTxn, write};
