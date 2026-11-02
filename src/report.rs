//! The report: sections of metrics and findings, serialized as JSON and rendered as HTML.
//!
//! The report is deterministic: the same inputs give byte-identical JSON. It therefore carries no
//! timestamp or timing; run time is printed to stderr only.

use serde::Serialize;
use serde_json::Value;

use crate::model::{InputCheck, Origin, RecordedFinding};

#[derive(Debug, Serialize)]
pub struct Report {
    pub tool: &'static str,
    pub version: &'static str,
    pub inputs: Vec<String>,
    pub sections: Vec<Section>,
    /// Self-checks carried by the inputs (e.g. the review desk's own check).
    pub input_checks: Vec<InputCheck>,
    /// Problems the project record already lists, kept next to the report's own findings.
    pub recorded: Vec<RecordedFinding>,
}

#[derive(Debug, Serialize)]
pub struct Section {
    pub id: &'static str,
    pub title: &'static str,
    pub ran: bool,
    pub notes: Vec<String>,
    pub metrics: Vec<Metric>,
    pub findings: Vec<Finding>,
}

#[derive(Debug, Serialize)]
pub struct Metric {
    pub name: String,
    pub value: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub check: &'static str,
    pub element: String,
    pub origin: Origin,
    /// Source ontology or section of a candidate term.
    pub source: Option<String>,
    pub status: Option<String>,
    pub detail: String,
}

impl Section {
    pub fn new(id: &'static str, title: &'static str) -> Self {
        Section {
            id,
            title,
            ran: true,
            notes: Vec::new(),
            metrics: Vec::new(),
            findings: Vec::new(),
        }
    }

    pub fn not_run(id: &'static str, title: &'static str, why: impl Into<String>) -> Self {
        let mut s = Section::new(id, title);
        s.ran = false;
        s.notes.push(why.into());
        s
    }

    pub fn metric(&mut self, name: impl Into<String>, value: impl Into<Value>) {
        self.metrics.push(Metric {
            name: name.into(),
            value: value.into(),
        });
    }

    /// Sort findings so the output does not depend on input order beyond what the input fixes.
    pub fn finish(mut self) -> Self {
        self.findings.sort_by(|a, b| {
            (a.check, &a.element, a.origin as u8).cmp(&(b.check, &b.element, b.origin as u8))
        });
        self
    }

    pub fn count(&self, check: &str) -> usize {
        self.findings.iter().filter(|f| f.check == check).count()
    }
}
