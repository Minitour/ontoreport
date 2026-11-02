//! The report: an overview, then sections of metrics, charts and findings. Serialized as JSON and
//! rendered as HTML.
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
    pub overview: Overview,
    pub sections: Vec<Section>,
    /// Self-checks carried by the inputs (e.g. the review desk's own check).
    pub input_checks: Vec<InputCheck>,
    /// Problems the project record already lists, kept next to the report's own findings.
    pub recorded: Vec<RecordedFinding>,
}

#[derive(Debug, Serialize)]
pub struct Overview {
    pub kpis: Vec<Kpi>,
    /// Findings by section, then check, then source.
    pub findings: Chart,
}

#[derive(Debug, Serialize)]
pub struct Kpi {
    pub label: String,
    /// The headline value, already formatted ("94.1%", "12,712").
    pub value: String,
    pub detail: String,
    pub tone: Tone,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help: Option<&'static str>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Tone {
    Good,
    Warn,
    Bad,
    Neutral,
}

/// Chart data, computed by the sections so that agents get it in the JSON and the HTML only draws it.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Chart {
    /// One bar per item, largest first.
    Bars { title: String, items: Vec<Item> },
    /// One row per label, each split into the same series.
    Stacked {
        title: String,
        series: Vec<String>,
        rows: Vec<Row>,
    },
    /// A part of a whole, drawn as a donut.
    Ratio {
        title: String,
        part: u64,
        whole: u64,
        /// Whether a high share is good (coverage), bad (errors) or neither, for the colour.
        direction: Direction,
    },
    /// A hierarchy whose nodes all carry the same series, drawn as a drill-down table.
    Tree {
        title: String,
        series: Vec<String>,
        roots: Vec<Node>,
    },
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    HigherIsBetter,
    LowerIsBetter,
    Neutral,
}

#[derive(Debug, Serialize)]
pub struct Item {
    pub label: String,
    pub value: u64,
}

#[derive(Debug, Serialize)]
pub struct Row {
    pub label: String,
    pub values: Vec<u64>,
}

#[derive(Debug, Serialize)]
pub struct Node {
    pub label: String,
    pub values: Vec<u64>,
    pub children: Vec<Node>,
}

#[derive(Debug, Serialize)]
pub struct Section {
    pub id: &'static str,
    pub title: &'static str,
    pub ran: bool,
    pub notes: Vec<String>,
    pub metrics: Vec<Metric>,
    pub charts: Vec<Chart>,
    pub findings: Vec<Finding>,
}

#[derive(Debug, Serialize)]
pub struct Metric {
    pub name: String,
    pub value: Value,
    /// What the metric is and how it is measured, in plain words.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help: Option<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub check: String,
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
            charts: Vec::new(),
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
        let name = name.into();
        self.metrics.push(Metric {
            help: crate::help::about(&name),
            name,
            value: value.into(),
        });
    }

    pub fn metric_value(&self, name: &str) -> Option<&Value> {
        self.metrics
            .iter()
            .find(|m| m.name == name)
            .map(|m| &m.value)
    }

    /// Sort findings so the output does not depend on input order beyond what the input fixes.
    pub fn finish(mut self) -> Self {
        self.findings.sort_by(|a, b| {
            (&a.check, &a.element, a.origin as u8).cmp(&(&b.check, &b.element, b.origin as u8))
        });
        self
    }

    pub fn count(&self, check: &str) -> usize {
        self.findings.iter().filter(|f| f.check == check).count()
    }
}

/// Bars from (label, count) pairs: largest first, ties by label, at most `top` items (0 = all).
pub fn bars<I: IntoIterator<Item = (String, u64)>>(
    title: impl Into<String>,
    counts: I,
    top: usize,
) -> Chart {
    let mut items: Vec<Item> = counts
        .into_iter()
        .filter(|(_, v)| *v > 0)
        .map(|(label, value)| Item { label, value })
        .collect();
    items.sort_by(|a, b| b.value.cmp(&a.value).then_with(|| a.label.cmp(&b.label)));
    if top > 0 {
        items.truncate(top);
    }
    Chart::Bars {
        title: title.into(),
        items,
    }
}
