//! UniFormat core: the data model, interfaces and utilities shared by every format crate.
//!
//! - The common data model is **Apache Arrow** (`RecordBatch`, `Schema`); format-specific metadata lives in the
//!   schema metadata under the prefix `<format>.`.
//! - [`Format`] is the interface a format implements; [`Registry`] finds a format by name or extension;
//!   [`convert()`] moves a table from one format to another through Arrow.
//! - [`ByteSource`] (range reads) and [`AtomicFile`] (transactional writes) are the I/O building blocks.
//!
//! This crate depends on **no format crate**; format crates depend on it.
#![forbid(unsafe_code)]

mod atomic;
mod convert;
mod error;
mod format;
mod registry;
mod source;

pub use atomic::AtomicFile;
pub use convert::convert;
pub use error::{Error, Result};
pub use format::{BatchStream, Format, ReadRequest, TableInfo, WriteRequest, WriteSummary};
pub use registry::Registry;
pub use source::{ByteSource, CountingSource, FileSource, SliceSource};
