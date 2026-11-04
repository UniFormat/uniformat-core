//! Format-to-format conversion through the common (Arrow) data model.

use std::path::Path;

use crate::error::Result;
use crate::format::{ReadRequest, WriteRequest, WriteSummary};
use crate::registry::Registry;

/// Converts `src` to `dst`: the formats are chosen by extension unless named, the data goes through Arrow.
///
/// # Errors
/// Unknown format, read error or write error. The destination is untouched on error (transactional write).
pub fn convert(
    registry: &Registry,
    src: &Path,
    dst: &Path,
    src_format: Option<&str>,
    dst_format: Option<&str>,
    read: &ReadRequest,
    write: &WriteRequest,
) -> Result<WriteSummary> {
    let from = match src_format {
        Some(n) => registry.by_name(n)?,
        None => registry.by_path(src)?,
    };
    let to = match dst_format {
        Some(n) => registry.by_name(n)?,
        None => registry.by_path(dst)?,
    };
    let stream = from.read(src, read)?;
    let schema = arrow_array::RecordBatchReader::schema(&*stream);
    to.write(dst, schema, stream, write)
}
