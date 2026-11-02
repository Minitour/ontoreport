//! Report sections that need only the index (no network, no reasoner).
//!
//! Scope: statement terms (what is in an ontology file) are always checked; candidate terms on a review
//! desk are checked unless their published status is "out", since those will not enter the ontology.

use std::collections::{BTreeMap, HashMap};

use crate::model::{Index, Origin, Term};
use crate::report::{Finding, Section};

const TOP_LABELS: &[&str] = &["entity", "owl:thing", "thing"];
const WEAK_SOURCES: &[&str] = &["wikipedia.org", "wikipedia"];

fn in_scope(t: &Term) -> bool {
    match t.origin {
        Origin::Statements => true,
        Origin::Candidates => t.group.as_ref().and_then(|g| g.status.as_deref()) != Some("out"),
    }
}

fn status(t: &Term) -> Option<String> {
    t.group.as_ref().and_then(|g| g.status.clone())
}

fn origin_name(o: Origin) -> &'static str {
    match o {
        Origin::Statements => "ontology file",
        Origin::Candidates => "desk candidates",
    }
}

// ── Structure: classes without a path to the top class ────────────────────

#[derive(Clone, Copy, PartialEq)]
enum Reach {
    Visiting,
    Yes,
    No,
}

struct Tree<'a> {
    parents: HashMap<&'a str, Vec<&'a str>>,
    memo: HashMap<&'a str, Reach>,
}

impl<'a> Tree<'a> {
    /// Parent links by lower-cased label, from every term the project knows plus the extra parent table.
    /// A file that imports another ontology has its parents there, so parents resolve across origins.
    fn new(index: &'a Index) -> Self {
        let mut parents: HashMap<&str, Vec<&str>> = HashMap::new();
        for (label, ps) in &index.extra_parents {
            parents
                .entry(label.as_str())
                .or_default()
                .extend(ps.iter().map(String::as_str));
        }
        for t in &index.terms {
            let e = parents.entry(t.key.as_str()).or_default();
            e.extend(t.parents.iter().map(String::as_str));
        }
        Tree {
            parents,
            memo: HashMap::new(),
        }
    }

    fn reaches_top(&mut self, label: &'a str) -> bool {
        if TOP_LABELS.contains(&label) {
            return true;
        }
        match self.memo.get(label) {
            Some(Reach::Yes) => return true,
            Some(Reach::No) | Some(Reach::Visiting) => return false,
            None => {}
        }
        self.memo.insert(label, Reach::Visiting);
        let ps = self.parents.get(label).cloned().unwrap_or_default();
        let ok = ps.into_iter().any(|p| self.reaches_top(p));
        self.memo
            .insert(label, if ok { Reach::Yes } else { Reach::No });
        ok
    }

    /// Labels on the way up that have no parents at all (where the chain breaks), at most `n`.
    fn dead_ends(&self, label: &'a str, n: usize) -> Vec<&'a str> {
        let mut out = Vec::new();
        let mut stack = vec![label];
        let mut seen = std::collections::HashSet::new();
        while let Some(x) = stack.pop() {
            if !seen.insert(x) || TOP_LABELS.contains(&x) {
                continue;
            }
            match self.parents.get(x) {
                Some(ps) if !ps.is_empty() => stack.extend(ps.iter().copied()),
                _ if x != label => out.push(x),
                _ => {}
            }
        }
        out.sort_unstable();
        out.truncate(n);
        out
    }
}

pub fn structure(index: &Index) -> Section {
    let mut s = Section::new("structure", "Structure");
    let mut tree = Tree::new(index);
    for origin in [Origin::Statements, Origin::Candidates] {
        if !index.has(origin) {
            continue;
        }
        let classes: Vec<&Term> = index
            .terms
            .iter()
            .filter(|t| t.origin == origin && t.kind == "class" && in_scope(t))
            .collect();
        let mut missing = 0;
        for t in &classes {
            let key = t.key.as_str();
            if !tree.reaches_top(key) {
                missing += 1;
                let ends = tree.dead_ends(key, 3);
                s.findings.push(Finding {
                    check: "no path to the top class",
                    element: t.label.clone(),
                    origin,
                    source: t.source.clone(),
                    status: status(t),
                    detail: if ends.is_empty() {
                        "no parent recorded".into()
                    } else {
                        format!("chain breaks at: {}", ends.join(", "))
                    },
                });
            }
        }
        s.metric(
            format!("classes checked ({})", origin_name(origin)),
            classes.len(),
        );
        s.metric(
            format!(
                "classes without a path to the top class ({})",
                origin_name(origin)
            ),
            missing,
        );
    }
    s.notes.push(
        "Parents are resolved by label through every term the project knows (ontology files, desk candidates \
         and the desk's parent-chain table), since a file's parents may sit in an ontology it imports. \
         Candidates marked out are not checked."
            .into(),
    );
    if !index.recorded.is_empty() {
        s.metric("tree errors recorded by the desk", index.recorded.len());
        s.notes.push(
            "The desk's recorded tree errors were computed against the full source releases; this check uses \
             only the parents published with the desk, so the two lists are compared rather than expected to match."
                .into(),
        );
    }
    s.notes
        .push("Consistency and pitfalls need an ontology file and run in a later version.".into());
    s.finish()
}

// ── Provenance: definitions and their sources ─────────────────────────────

pub fn provenance(index: &Index) -> Section {
    let mut s = Section::new("provenance", "Provenance");
    for origin in [Origin::Statements, Origin::Candidates] {
        if !index.has(origin) {
            continue;
        }
        let terms: Vec<&Term> = index
            .terms
            .iter()
            .filter(|t| t.origin == origin && in_scope(t) && t.kind != "individual")
            .collect();
        let (mut no_def, mut with_src, mut weak) = (0, 0, 0);
        for t in &terms {
            if t.definitions.is_empty() {
                no_def += 1;
                s.findings.push(finding("no definition", t, String::new()));
                continue;
            }
            if !t.definition_sources.is_empty() {
                with_src += 1;
            }
            let hay = t
                .definitions
                .iter()
                .chain(t.definition_sources.iter())
                .map(|x| x.to_lowercase())
                .collect::<Vec<_>>()
                .join(" ");
            if let Some(w) = WEAK_SOURCES.iter().find(|w| hay.contains(*w)) {
                weak += 1;
                s.findings.push(finding(
                    "definition cites a weak source",
                    t,
                    format!("mentions {w}"),
                ));
            }
        }
        let o = origin_name(origin);
        s.metric(format!("terms checked ({o})"), terms.len());
        s.metric(format!("without a definition ({o})"), no_def);
        s.metric(
            format!("definitions with their own source annotation ({o})"),
            with_src,
        );
        s.metric(format!("citing a weak source ({o})"), weak);
    }
    s.notes.push(
        "BFO elucidations count as definitions. A reused term's definition is quoted from its source release, \
         which the record names; the source ontology's own definition-source annotation is counted, not \
         required. Locators (page or section) and the bibliography are checked in a later version."
            .into(),
    );
    s.finish()
}

fn finding(check: &'static str, t: &Term, detail: String) -> Finding {
    Finding {
        check,
        element: t.label.clone(),
        origin: t.origin,
        source: t.source.clone(),
        status: status(t),
        detail: match (&t.curie, detail.is_empty()) {
            (Some(c), true) => c.clone(),
            (Some(c), false) => format!("{c}: {detail}"),
            (None, _) => detail,
        },
    }
}

// ── Traceability: what each kept group says it serves ─────────────────────

/// Requirement identifiers as written in the project record: one or two capital letters and a number
/// (A1, B12, X3, CQ7). Anything else in the text is ignored.
pub fn requirement_ids(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| {
            let letters = w.chars().take_while(|c| c.is_ascii_uppercase()).count();
            (1..=2).contains(&letters)
                && w.len() > letters
                && w[letters..].chars().all(|c| c.is_ascii_digit())
        })
        .map(str::to_string)
        .collect()
}

pub fn traceability(index: &Index) -> Section {
    if !index.has(Origin::Candidates) {
        return Section::not_run(
            "traceability",
            "Traceability",
            "needs requirement links: desk groups, or a requirements file (later version)",
        );
    }
    let mut s = Section::new("traceability", "Traceability");
    let mut groups: BTreeMap<&str, (&Term, usize)> = BTreeMap::new();
    for t in index
        .terms
        .iter()
        .filter(|t| t.origin == Origin::Candidates && in_scope(t))
    {
        if let Some(g) = &t.group {
            groups.entry(g.id.as_str()).or_insert((t, 0)).1 += 1;
        }
    }
    let mut per_req: BTreeMap<String, usize> = BTreeMap::new();
    let (mut with_req, mut with_reason_only, mut with_none) = (0, 0, 0);
    for (t, n) in groups.values() {
        let g = t.group.as_ref().expect("candidate term has a group");
        let ids = g.serves.as_deref().map(requirement_ids).unwrap_or_default();
        if !ids.is_empty() {
            with_req += 1;
            for id in ids {
                *per_req.entry(id).or_default() += 1;
            }
        } else if g.why.is_some() || g.serves.is_some() {
            with_reason_only += 1;
        } else {
            with_none += 1;
            s.findings.push(Finding {
                check: "group with no question, use case or reason",
                element: g.name.clone(),
                origin: Origin::Candidates,
                source: t.source.clone(),
                status: g.status.clone(),
                detail: format!("{} ({n} terms)", g.id),
            });
        }
    }
    s.metric("groups checked (not out)", groups.len());
    s.metric("groups naming a competency question or use case", with_req);
    s.metric("groups with a reason but no question", with_reason_only);
    s.metric("groups with neither", with_none);
    s.metric("distinct requirements named", per_req.len());
    s.notes.push(
        "Traceability is recorded per group on the desk. Requirements are read from the group's 'serves' text \
         (identifiers such as A1, B12, X3)."
            .into(),
    );
    s.finish()
}

// ── Review status ──────────────────────────────────────────────────────────

pub fn review(index: &Index) -> Section {
    if !index.has(Origin::Candidates) {
        return Section::not_run(
            "review",
            "Review status",
            "needs review records (desk groups or a decisions export)",
        );
    }
    let mut s = Section::new("review", "Review status");
    let mut table: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    for t in index
        .terms
        .iter()
        .filter(|t| t.origin == Origin::Candidates)
    {
        let src = t.source.clone().unwrap_or_else(|| "(unknown)".into());
        let st = status(t).unwrap_or_else(|| "none".into());
        *table.entry(src).or_default().entry(st).or_default() += 1;
    }
    for (src, counts) in &table {
        s.metric(
            format!("terms by status: {src}"),
            serde_json::to_value(counts).expect("map serializes"),
        );
    }
    s.notes.push(
        "Statuses are those published with the desk, which may be the assistant's proposals; the reviewer's own \
         decisions are read from a decisions export when one is given (later version)."
            .into(),
    );
    s.finish()
}
