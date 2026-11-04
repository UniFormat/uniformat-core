//! Registry of formats, looked up by name or by file extension.

use std::path::Path;
use std::sync::Arc;

use crate::error::{Error, Result};
use crate::format::Format;

/// A set of registered formats.
#[derive(Default, Clone)]
pub struct Registry {
    formats: Vec<Arc<dyn Format>>,
}

impl std::fmt::Debug for Registry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list()
            .entries(self.formats.iter().map(|x| x.name()))
            .finish()
    }
}

impl Registry {
    /// An empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a format. A format registered later replaces an earlier one with the same name.
    pub fn register(&mut self, format: Arc<dyn Format>) -> &mut Self {
        self.formats.retain(|f| f.name() != format.name());
        self.formats.push(format);
        self
    }

    /// Registered formats, in registration order.
    #[must_use]
    pub fn formats(&self) -> &[Arc<dyn Format>] {
        &self.formats
    }

    /// The format called `name` (case-insensitive).
    ///
    /// # Errors
    /// [`Error::UnknownFormat`].
    pub fn by_name(&self, name: &str) -> Result<Arc<dyn Format>> {
        self.formats
            .iter()
            .find(|f| f.name().eq_ignore_ascii_case(name))
            .cloned()
            .ok_or_else(|| Error::UnknownFormat(name.to_owned()))
    }

    /// The format that owns the extension of `path` (case-insensitive).
    ///
    /// # Errors
    /// [`Error::UnknownFormat`] when the path has no extension or no format claims it.
    pub fn by_path(&self, path: &Path) -> Result<Arc<dyn Format>> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .ok_or_else(|| Error::UnknownFormat(format!("{} (no extension)", path.display())))?;
        self.formats
            .iter()
            .find(|f| f.extensions().iter().any(|e| e.eq_ignore_ascii_case(&ext)))
            .cloned()
            .ok_or_else(|| Error::UnknownFormat(format!(".{ext}")))
    }
}
