//! Report sections that need only the index (no network, no reasoner).
//!
//! Scope: statement terms (what is in an ontology file) are always checked; candidate terms on a review
//! desk are checked unless their published status is "out", since those will not enter the ontology.
//! Each section returns metrics, chart data and findings.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::model::{Index, Origin, Term};
use crate::releases::{ReleaseTerm, Releases};
use crate::report::{bars, Chart, Direction, Finding, Node, Row, Section};

/// Keys of the top class: labels, or the IRI (lower-cased) when the file defining it is not loaded.
const TOP_LABELS: &[&str] = &[
    "entity",
    "owl:thing",
    "thing",
    "http://purl.obolibrary.org/obo/bfo_0000001",
];
const WEAK_SOURCES: &[&str] = &["wikipedia.org", "wikipedia"];

/// Review statuses in display order; anything else is grouped under "other".
pub const STATUSES: &[&str] = &["in", "debatable", "out", "other"];

/// Whether a term is checked: everything except desk candidates marked Out, unless the index asks for
/// those too (`--include-out`).
pub(crate) fn in_scope(index: &Index, t: &Term) -> bool {
    match t.origin {
        Origin::Statements => true,
        Origin::Candidates => {
            index.include_out || t.group.as_ref().and_then(|g| g.status.as_deref()) != Some("out")
        }
    }
}

pub(crate) fn status(t: &Term) -> Option<String> {
    t.group.as_ref().and_then(|g| g.status.clone())
}

fn status_slot(t: &Term) -> usize {
    match t.group.as_ref().and_then(|g| g.status.as_deref()) {
        Some("in") => 0,
        Some("debatable") => 1,
        Some("out") => 2,
        _ => 3,
    }
}

pub fn origin_name(o: Origin) -> &'static str {
    match o {
        Origin::Statements => "ontology file",
        Origin::Candidates => "desk candidates",
    }
}

/// Where a term comes from, for breakdowns: its source on the desk, or "ontology file".
fn place(t: &Term) -> String {
    match (t.origin, &t.source) {
        (Origin::Candidates, Some(s)) => s.clone(),
        (Origin::Candidates, None) => "desk (unknown source)".into(),
        (Origin::Statements, _) => "ontology file".into(),
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
    /// Without BFO in the project, a class with no recorded parent is a root under owl:Thing.
    roots_reach_top: bool,
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
        let roots_reach_top = !uses_bfo(index);
        Tree {
            parents,
            memo: HashMap::new(),
            roots_reach_top,
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
        let ok = if ps.is_empty() {
            self.roots_reach_top && self.parents.contains_key(label)
        } else {
            ps.into_iter().any(|p| self.reaches_top(p))
        };
        self.memo
            .insert(label, if ok { Reach::Yes } else { Reach::No });
        ok
    }

    /// Labels on the way up that have no parents at all (where the chain breaks), at most `n`.
    fn dead_ends(&self, label: &'a str, n: usize) -> Vec<&'a str> {
        let mut out = Vec::new();
        let mut stack = vec![label];
        let mut seen = HashSet::new();
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

/// The tree as the source releases draw it: a reused class's parents as its release states them, so a
/// chain is followed through terms the project did not choose. An obsolete class, or a parent no release
/// read contains, ends the chain. This is the check the review desk ran against the full releases.
struct ReleaseTree<'a> {
    rels: &'a Releases,
    memo: HashMap<&'a str, Reach>,
}

const BFO_ENTITY_IRI: &str = "http://purl.obolibrary.org/obo/BFO_0000001";

impl<'a> ReleaseTree<'a> {
    fn term(&self, iri: &str) -> Option<&'a ReleaseTerm> {
        self.rels.find(iri, None).and_then(|(_, t)| t)
    }

    fn knows(&self, iri: &str) -> bool {
        self.term(iri).is_some()
    }

    fn reaches_top(&mut self, iri: &'a str) -> bool {
        if iri == BFO_ENTITY_IRI {
            return true;
        }
        match self.memo.get(iri) {
            Some(Reach::Yes) => return true,
            Some(Reach::No) | Some(Reach::Visiting) => return false,
            None => {}
        }
        self.memo.insert(iri, Reach::Visiting);
        let ok = match self.term(iri) {
            Some(t) if !t.deprecated => t.parents.iter().any(|p| self.reaches_top(p)),
            _ => false,
        };
        self.memo
            .insert(iri, if ok { Reach::Yes } else { Reach::No });
        ok
    }

    /// Where the chains end on the way up: classes with no parent, obsolete classes, and parents that no
    /// release contains, at most `n`, by name.
    fn dead_ends(&self, iri: &'a str, n: usize) -> Vec<String> {
        let name = |x: &str| match self.term(x).and_then(ReleaseTerm::label) {
            Some(l) => l.to_string(),
            None => crate::reuse::curie(x),
        };
        let mut out = BTreeSet::new();
        let mut stack = vec![iri];
        let mut seen = HashSet::new();
        while let Some(x) = stack.pop() {
            if !seen.insert(x) || x == BFO_ENTITY_IRI {
                continue;
            }
            match self.term(x) {
                None => {
                    out.insert(format!("{} (in no release read)", name(x)));
                }
                Some(t) if t.deprecated && x != iri => {
                    out.insert(format!("{} (obsolete)", name(x)));
                }
                Some(t) if t.parents.is_empty() => {
                    if x != iri {
                        out.insert(name(x));
                    }
                }
                Some(t) => stack.extend(t.parents.iter().map(String::as_str)),
            }
        }
        out.into_iter().take(n).collect()
    }
}

/// Whether the project is built on BFO: then every class must reach BFO's 'entity'. Otherwise the top
/// is owl:Thing and any class with no asserted parent is a root under it.
fn uses_bfo(index: &Index) -> bool {
    const BFO_ENTITY: &str = "http://purl.obolibrary.org/obo/bfo_0000001";
    index.terms.iter().any(|t| {
        t.iri.as_deref() == Some("http://purl.obolibrary.org/obo/BFO_0000001")
            || t.parents.iter().any(|p| p == "entity" || p == BFO_ENTITY)
    }) || index
        .extra_parents
        .values()
        .flatten()
        .any(|p| p == "entity")
}

pub fn structure(index: &Index) -> Section {
    structure_with(index, None)
}

/// The structure section; with source releases, reused classes are followed through their releases.
pub fn structure_with(index: &Index, releases: Option<&Releases>) -> Section {
    let mut s = Section::new("structure", "Structure");
    let mut tree = Tree::new(index);
    let mut rtree = releases
        .filter(|_| !tree.roots_reach_top)
        .map(|rels| ReleaseTree {
            rels,
            memo: HashMap::new(),
        });
    let mut by_release = 0usize;
    s.metric(
        "top class",
        if tree.roots_reach_top {
            "owl:Thing"
        } else {
            "BFO entity"
        },
    );
    let mut breaks: BTreeMap<String, u64> = BTreeMap::new();
    let mut missing_by: BTreeMap<String, u64> = BTreeMap::new();
    for origin in [Origin::Statements, Origin::Candidates] {
        if !index.has(origin) {
            continue;
        }
        let classes: Vec<&Term> = index
            .terms
            .iter()
            .filter(|t| t.origin == origin && t.kind == "class" && in_scope(index, t))
            .collect();
        let mut missing = 0;
        for t in &classes {
            // A desk candidate its release contains is followed through the releases; anything else
            // (the project's own classes, and candidates from sources no release was read for) through
            // the parents the project records.
            let in_release = match (&mut rtree, t.iri.as_deref()) {
                (Some(rt), Some(iri)) if origin == Origin::Candidates && rt.knows(iri) => {
                    Some((rt, iri))
                }
                _ => None,
            };
            let (ok, ends, where_) = match in_release {
                Some((rt, iri)) => {
                    by_release += 1;
                    let ok = rt.reaches_top(iri);
                    let ends = if ok { Vec::new() } else { rt.dead_ends(iri, 3) };
                    (ok, ends, "in the source releases, ")
                }
                None => {
                    let key = t.key.as_str();
                    let ok = tree.reaches_top(key);
                    let ends = if ok {
                        Vec::new()
                    } else {
                        tree.dead_ends(key, 3)
                            .into_iter()
                            .map(str::to_string)
                            .collect()
                    };
                    (ok, ends, "")
                }
            };
            if !ok {
                missing += 1;
                *missing_by.entry(place(t)).or_default() += 1;
                for e in &ends {
                    *breaks.entry(e.clone()).or_default() += 1;
                }
                s.findings.push(Finding {
                    check: "no path to the top class".into(),
                    element: t.label.clone(),
                    origin,
                    source: t.source.clone(),
                    status: status(t),
                    detail: if ends.is_empty() {
                        format!("{where_}no parent recorded")
                    } else {
                        format!("{where_}the chain breaks at: {}", ends.join(", "))
                    },
                });
            }
        }
        let o = origin_name(origin);
        s.metric(format!("classes checked ({o})"), classes.len());
        s.metric(
            format!("classes without a path to the top class ({o})"),
            missing,
        );
        s.charts.push(Chart::Ratio {
            title: format!("Classes reaching the top class ({o})"),
            part: (classes.len() - missing) as u64,
            whole: classes.len() as u64,
            direction: Direction::HigherIsBetter,
        });
    }
    s.charts
        .push(bars("Classes without a path, by source", missing_by, 0));
    s.charts
        .push(bars("Where parent chains break (top 12)", breaks, 12));
    if by_release > 0 {
        s.notes.push(format!(
            "{} desk candidates were followed through their source releases: parents as each release states \
             them, with an obsolete class or a parent in no release read ending the chain.",
            crate::thousands(by_release as u64)
        ));
    }
    s.notes.push(
        "Other classes are followed by label through every term the project knows (ontology files, desk \
         candidates and the desk's parent-chain table), since a file's parents may sit in an ontology it \
         imports. Candidates marked out are not checked."
            .into(),
    );
    if !index.recorded.is_empty() {
        s.metric("tree errors recorded by the desk", index.recorded.len());
        s.notes.push(
            "The desk records its own tree errors, including candidates marked out; they are kept for comparison."
                .into(),
        );
    }
    s.finish()
}

// ── Provenance: definitions and their sources ─────────────────────────────

pub fn provenance(index: &Index) -> Section {
    let mut s = Section::new("provenance", "Provenance");
    let mut missing_by: BTreeMap<String, u64> = BTreeMap::new();
    let mut weak_by: BTreeMap<String, u64> = BTreeMap::new();
    for origin in [Origin::Statements, Origin::Candidates] {
        if !index.has(origin) {
            continue;
        }
        let terms: Vec<&Term> = index
            .terms
            .iter()
            .filter(|t| t.origin == origin && in_scope(index, t) && t.kind != "individual")
            .collect();
        let (mut no_def, mut with_src, mut weak) = (0u64, 0u64, 0u64);
        for t in &terms {
            if t.definitions.is_empty() {
                no_def += 1;
                *missing_by.entry(place(t)).or_default() += 1;
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
                *weak_by.entry(place(t)).or_default() += 1;
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
        s.charts.push(Chart::Ratio {
            title: format!("Terms with a definition ({o})"),
            part: terms.len() as u64 - no_def,
            whole: terms.len() as u64,
            direction: Direction::HigherIsBetter,
        });
    }
    s.charts
        .push(bars("Terms without a definition, by source", missing_by, 0));
    s.charts.push(bars(
        "Definitions citing a weak source, by source",
        weak_by,
        0,
    ));
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
        check: check.into(),
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
        .filter(|t| t.origin == Origin::Candidates && in_scope(index, t))
    {
        if let Some(g) = &t.group {
            groups.entry(g.id.as_str()).or_insert((t, 0)).1 += 1;
        }
    }
    let mut per_req: BTreeMap<String, u64> = BTreeMap::new();
    // Per source: groups naming a requirement, with a reason only, with neither.
    let mut by_source: BTreeMap<String, [u64; 3]> = BTreeMap::new();
    let (mut with_req, mut with_reason_only, mut with_none) = (0u64, 0u64, 0u64);
    for (t, n) in groups.values() {
        let g = t.group.as_ref().expect("candidate term has a group");
        let ids = g.serves.as_deref().map(requirement_ids).unwrap_or_default();
        let slot = by_source.entry(place(t)).or_default();
        if !ids.is_empty() {
            with_req += 1;
            slot[0] += 1;
            for id in ids {
                *per_req.entry(id).or_default() += 1;
            }
        } else if g.why.is_some() || g.serves.is_some() {
            with_reason_only += 1;
            slot[1] += 1;
        } else {
            with_none += 1;
            slot[2] += 1;
            s.findings.push(Finding {
                check: "group with no question, use case or reason".into(),
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
    s.charts.push(Chart::Ratio {
        title: "Groups naming a requirement".into(),
        part: with_req,
        whole: groups.len() as u64,
        direction: Direction::HigherIsBetter,
    });
    s.charts.push(Chart::Stacked {
        title: "How each group is justified, by source".into(),
        series: vec![
            "names a requirement".into(),
            "reason only".into(),
            "neither".into(),
        ],
        rows: by_source
            .into_iter()
            .map(|(label, v)| Row {
                label,
                values: v.to_vec(),
            })
            .collect(),
    });
    s.charts.push(bars(
        "Groups serving each requirement (top 15)",
        per_req,
        15,
    ));
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
    // source -> category -> group -> counts per status
    type Counts = [u64; 4];
    let mut tree: BTreeMap<String, BTreeMap<String, BTreeMap<String, Counts>>> = BTreeMap::new();
    for t in index
        .terms
        .iter()
        .filter(|t| t.origin == Origin::Candidates)
    {
        let g = t.group.as_ref();
        let cat = g
            .and_then(|g| g.category.clone())
            .unwrap_or_else(|| "(no category)".into());
        let grp = g
            .map(|g| g.name.clone())
            .unwrap_or_else(|| "(no group)".into());
        tree.entry(place(t))
            .or_default()
            .entry(cat)
            .or_default()
            .entry(grp)
            .or_default()[status_slot(t)] += 1;
    }
    fn sum(a: &mut [u64; 4], b: &[u64; 4]) {
        for i in 0..4 {
            a[i] += b[i];
        }
    }
    let mut roots = Vec::new();
    let mut rows = Vec::new();
    let mut totals = [0u64; 4];
    for (src, cats) in &tree {
        let mut src_total = [0u64; 4];
        let mut cat_nodes = Vec::new();
        for (cat, groups) in cats {
            let mut cat_total = [0u64; 4];
            let mut group_nodes = Vec::new();
            for (grp, c) in groups {
                sum(&mut cat_total, c);
                group_nodes.push(Node {
                    label: grp.clone(),
                    values: c.to_vec(),
                    children: Vec::new(),
                });
            }
            group_nodes.sort_by(|a, b| {
                total(&b.values)
                    .cmp(&total(&a.values))
                    .then_with(|| a.label.cmp(&b.label))
            });
            sum(&mut src_total, &cat_total);
            cat_nodes.push(Node {
                label: cat.clone(),
                values: cat_total.to_vec(),
                children: group_nodes,
            });
        }
        cat_nodes.sort_by(|a, b| {
            total(&b.values)
                .cmp(&total(&a.values))
                .then_with(|| a.label.cmp(&b.label))
        });
        sum(&mut totals, &src_total);
        s.metric(
            format!("terms by status: {src}"),
            serde_json::to_value(named(&src_total)).expect("map serializes"),
        );
        rows.push(Row {
            label: src.clone(),
            values: src_total.to_vec(),
        });
        roots.push(Node {
            label: src.clone(),
            values: src_total.to_vec(),
            children: cat_nodes,
        });
    }
    rows.sort_by(|a, b| {
        total(&b.values)
            .cmp(&total(&a.values))
            .then_with(|| a.label.cmp(&b.label))
    });
    roots.sort_by(|a, b| {
        total(&b.values)
            .cmp(&total(&a.values))
            .then_with(|| a.label.cmp(&b.label))
    });
    let series: Vec<String> = STATUSES.iter().map(|x| x.to_string()).collect();
    s.charts.push(Chart::Ratio {
        title: "Candidate terms kept (in)".into(),
        part: totals[0],
        whole: total(&totals),
        direction: Direction::Neutral,
    });
    s.charts.push(Chart::Stacked {
        title: "Terms by status, by source".into(),
        series: series.clone(),
        rows,
    });
    s.charts.push(Chart::Tree {
        title: "Source › category › group".into(),
        series,
        roots,
    });
    s.notes.push(
        "Statuses are those published with the desk, which may be the assistant's proposals; the reviewer's own \
         decisions are read from a decisions export when one is given (later version)."
            .into(),
    );
    s.finish()
}

fn total(v: &[u64]) -> u64 {
    v.iter().sum()
}

fn named(c: &[u64; 4]) -> BTreeMap<&'static str, u64> {
    STATUSES
        .iter()
        .zip(c.iter())
        .filter(|(_, n)| **n > 0)
        .map(|(k, n)| (*k, *n))
        .collect()
}

// ── Consistency and pitfalls (ontology files) ──────────────────────────────

const IMPORTANCE: &[&str] = &["Critical", "Important", "Minor"];

pub fn logic(index: &Index) -> Section {
    let mut s = logic_checks(index);
    for (path, why) in &index.unreadable {
        s.ran = true;
        s.findings.push(Finding {
            check: "file could not be read".into(),
            element: path.rsplit(['/', '\\']).next().unwrap_or(path).to_string(),
            origin: Origin::Statements,
            source: None,
            status: None,
            detail: why.clone(),
        });
    }
    if !index.unreadable.is_empty() {
        s.metric(
            "ontology files that could not be read",
            index.unreadable.len(),
        );
    }
    let repaired: usize = index
        .ontologies
        .iter()
        .map(|o| o.declared_on_read + o.nominals_skipped)
        .sum();
    if repaired > 0 {
        let declared: usize = index.ontologies.iter().map(|o| o.declared_on_read).sum();
        let skipped: usize = index.ontologies.iter().map(|o| o.nominals_skipped).sum();
        s.notes.push(format!(
            "To read the files, {declared} IRIs used as classes without a class declaration were declared              classes, and {skipped} equivalences to enumerated classes (owl:oneOf) were skipped."
        ));
    }
    s.finish()
}

fn logic_checks(index: &Index) -> Section {
    if index.ontologies.is_empty() {
        return Section::not_run(
            "logic",
            "Consistency and pitfalls",
            "needs an ontology file (RDF/XML, OWL functional syntax, Turtle or N-Triples)",
        );
    }
    let mut s = Section::new("logic", "Consistency and pitfalls");
    let timing = std::env::var_os("ONTOREPORT_TIMING").is_some();
    let (cons, pits) = std::thread::scope(|scope| {
        let c = scope.spawn(|| {
            let t = std::time::Instant::now();
            let r = crate::owl::consistency(&index.ontologies);
            if timing {
                eprintln!(
                    "    consistency (whelk) {:.0} ms",
                    t.elapsed().as_secs_f64() * 1e3
                );
            }
            r
        });
        let p = scope.spawn(|| {
            let t = std::time::Instant::now();
            let r = crate::owl::scan_pitfalls(&index.ontologies);
            if timing {
                eprintln!("    pitfall scan {:.0} ms", t.elapsed().as_secs_f64() * 1e3);
            }
            r
        });
        (
            c.join().expect("reasoner thread"),
            p.join().expect("pitfall thread"),
        )
    });
    let names: HashMap<&str, &Term> = index
        .terms
        .iter()
        .filter(|t| t.origin == Origin::Statements)
        .filter_map(|t| t.iri.as_deref().map(|i| (i, t)))
        .collect();
    let name = |iri: &str| -> String {
        match names.get(iri) {
            Some(t) if t.label != iri => {
                format!("{} ({})", t.label, t.curie.as_deref().unwrap_or(iri))
            }
            _ => iri.to_string(),
        }
    };

    let axioms: usize = index
        .ontologies
        .iter()
        .map(|o| o.ontology.iter().count())
        .sum();
    let loaded: HashSet<&str> = index
        .ontologies
        .iter()
        .filter_map(|o| o.iri.as_deref())
        .collect();
    let mut missing_imports: Vec<&str> = index
        .ontologies
        .iter()
        .flat_map(|o| o.imports.iter().map(String::as_str))
        .filter(|i| !loaded.contains(i))
        .collect();
    missing_imports.sort_unstable();
    missing_imports.dedup();
    let classes = pits.summary.num_classes as u64;

    s.metric("ontology files read", index.ontologies.len());
    s.metric("axioms and annotations", axioms);
    s.metric(
        "consistent under OWL 2 EL",
        if cons.consistent { "yes" } else { "no" },
    );
    s.metric("unsatisfiable classes", cons.unsatisfiable.len());
    s.metric("imports not loaded", missing_imports.len());
    for u in &cons.unsatisfiable {
        s.findings.push(Finding {
            check: "unsatisfiable class".into(),
            element: name(u),
            origin: Origin::Statements,
            source: None,
            status: None,
            detail: "inferred equivalent to owl:Nothing (whelk)".into(),
        });
    }

    // Pitfalls: one finding per affected element, marked with any decision the project recorded.
    let mut by_importance: BTreeMap<&str, [u64; 2]> = BTreeMap::new();
    let mut by_pitfall: BTreeMap<String, u64> = BTreeMap::new();
    let (mut open, mut decided) = (0u64, 0u64);
    for p in &pits.pitfalls {
        let elements: Vec<&str> = if p.affected_elements.is_empty() {
            vec!["(ontology)"]
        } else {
            p.affected_elements.iter().map(String::as_str).collect()
        };
        for e in elements {
            let decision = index.decisions.iter().find(|d| d.covers(&p.id, e));
            let slot = by_importance
                .entry(importance_key(&p.importance))
                .or_default();
            match decision {
                Some(_) => {
                    decided += 1;
                    slot[1] += 1;
                }
                None => {
                    open += 1;
                    slot[0] += 1;
                }
            }
            *by_pitfall
                .entry(format!("{} {}", p.id, p.title))
                .or_default() += 1;
            s.findings.push(Finding {
                check: format!("pitfall {} · {}", p.id, p.title),
                element: name(e),
                origin: Origin::Statements,
                source: Some(p.importance.clone()),
                status: Some(
                    decision
                        .map(|d| d.decision.clone())
                        .unwrap_or_else(|| "open".into()),
                ),
                detail: match decision {
                    Some(d) if !d.reference.is_empty() => format!("{} ({})", d.reason, d.reference),
                    Some(d) => d.reason.clone(),
                    None => p.description.clone(),
                },
            });
        }
    }
    s.metric("pitfall instances, open", open);
    s.metric("pitfall instances, decided", decided);
    s.charts.push(Chart::Ratio {
        title: "Satisfiable classes".into(),
        part: classes.saturating_sub(cons.unsatisfiable.len() as u64),
        whole: classes,
        direction: Direction::HigherIsBetter,
    });
    s.charts.push(Chart::Stacked {
        title: "Pitfall instances by importance".into(),
        series: vec!["open".into(), "decided".into()],
        rows: IMPORTANCE
            .iter()
            .map(|i| Row {
                label: (*i).to_string(),
                values: by_importance.get(i).copied().unwrap_or_default().to_vec(),
            })
            .collect(),
    });
    s.charts.push(bars(
        "Pitfall instances by pitfall (top 15)",
        by_pitfall,
        15,
    ));
    if !missing_imports.is_empty() {
        s.notes.push(format!(
            "Imports not loaded (add their files to the project folder to include them): {}. Parents and \
             axioms in them are not seen, so consistency and the tree check cover only the files read.",
            missing_imports.join(", ")
        ));
    }
    s.notes.push(
        "Consistency is OWL 2 EL reasoning with whelk over all files read; the pitfalls are OWL-MCP's 31 \
         OOPS!-style checks, so the agent's tool and this report agree. Decisions come from decisions.json."
            .into(),
    );
    s.finish()
}

fn importance_key(s: &str) -> &'static str {
    IMPORTANCE
        .iter()
        .find(|i| i.eq_ignore_ascii_case(s))
        .copied()
        .unwrap_or("Minor")
}
