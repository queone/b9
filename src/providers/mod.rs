//! Provider adapters built on b9's validating transport boundary.

use std::error::Error;
use std::fmt;

pub mod espn;
pub mod mlb;
pub mod yahoo;

/// A contextual provider acquisition failure.
#[derive(Debug)]
pub struct ProviderError {
    operation: &'static str,
    detail: String,
    source: Option<Box<dyn Error + Send + Sync>>,
}

impl ProviderError {
    pub(crate) fn invalid(operation: &'static str, detail: impl Into<String>) -> Self {
        Self {
            operation,
            detail: detail.into(),
            source: None,
        }
    }

    pub(crate) fn operation(
        operation: &'static str,
        detail: impl Into<String>,
        source: impl Error + Send + Sync + 'static,
    ) -> Self {
        Self {
            operation,
            detail: detail.into(),
            source: Some(Box::new(source)),
        }
    }

    /// Return the failed provider operation.
    pub fn operation_name(&self) -> &'static str {
        self.operation
    }
}

impl fmt::Display for ProviderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}: {}; verify the provider response and retry",
            self.operation, self.detail
        )
    }
}

impl Error for ProviderError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source.as_deref().map(|error| error as _)
    }
}
