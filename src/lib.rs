//! ontoreport: a quality report for OWL ontologies built from the project record.
//!
//! This is a placeholder release that reserves the package name. The report itself
//! (structure, reuse fidelity, traceability, provenance, requirements coverage and
//! review status) is under development.

/// The crate version, as published.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Message printed when the report is requested from a placeholder release.
pub fn placeholder_message() -> String {
    format!(
        "ontoreport {VERSION} is a placeholder release; the quality report is under development.\n\
         See https://github.com/Minitour/ontoreport"
    )
}
