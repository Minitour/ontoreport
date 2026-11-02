//! The in-memory index every section reads: one record per term, from whichever readers ran.

use std::collections::HashMap;

use serde::Serialize;

/// Where a term record came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    /// A statement-level view of an ontology file (desk `g0.json`): what is actually in the file.
    Statements,
    /// A candidate on the review desk (desk `groups.json`): proposed for reuse, not yet in a file.
    Candidates,
}

#[derive(Debug, Clone, Serialize)]
pub struct Term {
    pub label: String,
    /// Lower-cased label, the key parents refer to.
    #[serde(skip)]
    pub key: String,
    pub curie: Option<String>,
    pub iri: Option<String>,
    /// "class", "object property", "annotation property", ... as the input names it.
    pub kind: String,
    pub definitions: Vec<String>,
    pub definition_sources: Vec<String>,
    /// Parent labels (lower-cased for lookup by `Index::by_label`).
    pub parents: Vec<String>,
    pub origin: Origin,
    /// Source ontology or section the term was taken from.
    pub source: Option<String>,
    pub release: Option<String>,
    pub group: Option<GroupRef>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GroupRef {
    pub id: String,
    pub name: String,
    /// Status as published on the desk ("in", "debatable", "out").
    pub status: Option<String>,
    /// Competency questions or use cases the group says it serves, as written.
    pub serves: Option<String>,
    pub why: Option<String>,
}

/// A problem the project record already lists (e.g. the desk's own tree errors), kept for comparison.
#[derive(Debug, Clone, Serialize)]
pub struct RecordedFinding {
    pub check: String,
    pub element: String,
    pub detail: String,
}

/// A self-check the input carries (e.g. the desk check), shown as context.
#[derive(Debug, Clone, Serialize)]
pub struct InputCheck {
    pub name: String,
    pub ok: bool,
    pub summary: String,
}

#[derive(Debug, Default)]
pub struct Index {
    pub terms: Vec<Term>,
    /// Parent links for labels that are not terms themselves (the desk's parent-chain table).
    pub extra_parents: HashMap<String, Vec<String>>,
    pub recorded: Vec<RecordedFinding>,
    pub input_checks: Vec<InputCheck>,
    pub inputs: Vec<String>,
}

impl Index {
    /// Lower-cased label -> indices of the terms with that label.
    pub fn by_label(&self) -> HashMap<String, Vec<usize>> {
        let mut map: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, t) in self.terms.iter().enumerate() {
            map.entry(t.label.to_lowercase()).or_default().push(i);
        }
        map
    }

    pub fn has(&self, origin: Origin) -> bool {
        self.terms.iter().any(|t| t.origin == origin)
    }
}
