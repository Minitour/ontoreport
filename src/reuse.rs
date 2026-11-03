//! Reuse fidelity: whether each reused term matches the source release it was taken from.

use std::collections::{BTreeMap, BTreeSet};

use crate::model::{Index, Origin, Term};
use crate::releases::{Release, ReleaseTerm, Releases};
use crate::report::{Chart, Direction, Finding, Row, Section};
use crate::sections::{in_scope, origin_name, status};

/// The languages' own vocabularies (rdfs:label, owl:Thing, xsd:string): used, not reused.
const BUILT_IN: [&str; 4] = [
    "http://www.w3.org/1999/02/22-rdf-syntax-ns#",
    "http://www.w3.org/2000/01/rdf-schema#",
    "http://www.w3.org/2002/07/owl#",
    "http://www.w3.org/2001/XMLSchema#",
];

/// Problems, in the order of the chart's series.
const PROBLEMS: [&str; 6] = [
    "obsolete in the source release",
    "not in the source release",
    "under an obsolete class in the source release",
    "label differs from the release",
    "definition differs from the release",
    "definition not in the release",
];

/// A label without the qualifier a review desk adds to tell same-named terms apart: "Agent (DwC)",
/// "Entity (PROV-O)", "Language (DwC 2)".
fn bare_label(label: &str) -> &str {
    let l = label.trim();
    match l.rfind(" (") {
        Some(i) if l.ends_with(')') && i > 0 => &l[..i],
        _ => l,
    }
}

/// Whether the project gives the term no name of its own: its label is its identifier ("dcterms:title",
/// "IAO:0000118", the IRI or its local name), as a desk shows a term the file does not label.
fn unnamed(t: &Term) -> bool {
    let Some(iri) = t.iri.as_deref() else {
        return false;
    };
    let l = t.label.trim();
    let local = iri.rsplit(['#', '/']).next().unwrap_or(iri);
    l == iri
        || l.eq_ignore_ascii_case(&curie(iri))
        || l == local
        || l.rsplit_once(':')
            .is_some_and(|(p, name)| !p.contains(' ') && name == local)
}

/// Lower-cased, with runs of white space as one space and no final full stop.
fn norm(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_end_matches('.')
        .trim_end()
        .to_lowercase()
}

fn short(s: &str, n: usize) -> String {
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.chars().count() <= n {
        s
    } else {
        format!("{}…", s.chars().take(n).collect::<String>())
    }
}

pub(crate) fn curie(iri: &str) -> String {
    if let Some(rest) = iri.strip_prefix("http://purl.obolibrary.org/obo/") {
        if let Some((p, id)) = rest.split_once('_') {
            return format!("{p}:{id}");
        }
    }
    iri.to_string()
}

fn named(rel: &Release, iri: &str) -> String {
    match rel.terms.get(iri).and_then(ReleaseTerm::label) {
        Some(l) => format!("'{l}' ({})", curie(iri)),
        None => curie(iri),
    }
}

/// The nearest obsolete classes above a term in its release: following parents up, stopping at each
/// obsolete class found. A term can hang from a retired class several levels up (ENVO classes under
/// OBI's obsolete planned process), which a check of the direct parent alone misses.
fn obsolete_above<'a>(rel: &'a Release, rt: &'a ReleaseTerm) -> Vec<&'a str> {
    let mut found = BTreeSet::new();
    let mut seen = BTreeSet::new();
    let mut stack: Vec<&str> = rt.parents.iter().map(String::as_str).collect();
    while let Some(p) = stack.pop() {
        if !seen.insert(p) {
            continue;
        }
        match rel.terms.get(p) {
            Some(pt) if pt.deprecated => {
                found.insert(p);
            }
            Some(pt) => stack.extend(pt.parents.iter().map(String::as_str)),
            None => {}
        }
    }
    found.into_iter().collect()
}

/// The problems one term has against its release, as (problem index, detail).
fn check(t: &Term, rel: &Release, rt: Option<&ReleaseTerm>) -> Vec<(usize, String)> {
    let Some(rt) = rt else {
        return vec![(
            1,
            format!(
                "{} is not in {}",
                t.iri.as_deref().map(curie).unwrap_or_default(),
                rel.name
            ),
        )];
    };
    let mut out = Vec::new();
    if rt.deprecated {
        let repl: Vec<String> = rt.replaced_by.iter().map(|r| named(rel, r)).collect();
        out.push((
            0,
            if repl.is_empty() {
                format!("marked obsolete in {}", rel.name)
            } else {
                format!(
                    "marked obsolete in {}; replaced by {}",
                    rel.name,
                    repl.join(", ")
                )
            },
        ));
    }
    let above = obsolete_above(rel, rt);
    if !above.is_empty() {
        out.push((
            2,
            format!(
                "in {}, it sits under the obsolete {}",
                rel.name,
                above
                    .iter()
                    .map(|p| named(rel, p))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ));
    }
    if !rt.deprecated && !rt.labels.is_empty() && !unnamed(t) {
        let mine = norm(&t.label);
        let bare = norm(bare_label(&t.label));
        if !rt.labels.iter().any(|l| norm(l) == mine || norm(l) == bare) {
            out.push((3, format!("the release calls it '{}'", rt.labels[0])));
        }
    }
    if let Some(d) = t.definitions.first().filter(|d| !d.trim().is_empty()) {
        let mine = norm(d);
        if rt.definitions.is_empty() {
            out.push((
                5,
                format!(
                    "the release gives no definition; the project's reads '{}'",
                    short(d, 140)
                ),
            ));
        } else if !rt.definitions.iter().any(|x| norm(x) == mine) {
            out.push((
                4,
                format!(
                    "project: '{}'; release: '{}'",
                    short(d, 120),
                    short(&rt.definitions[0], 120)
                ),
            ));
        }
    }
    out
}

pub fn reuse(index: &Index, releases: Option<&Releases>) -> Section {
    const ID: &str = "reuse";
    const TITLE: &str = "Reuse fidelity";
    let mut s = Section::new(ID, TITLE);

    // One identifier, two names: needs no release.
    let mut names: BTreeMap<(&str, Origin), BTreeSet<String>> = BTreeMap::new();
    let mut shown: BTreeMap<(&str, Origin), &Term> = BTreeMap::new();
    for t in &index.terms {
        if let Some(iri) = t.iri.as_deref() {
            names
                .entry((iri, t.origin))
                .or_default()
                .insert(t.label.trim().to_string());
            shown.entry((iri, t.origin)).or_insert(t);
        }
    }
    let mut two_names = 0;
    for ((iri, origin), labels) in &names {
        let distinct: BTreeSet<String> = labels.iter().map(|l| norm(bare_label(l))).collect();
        if distinct.len() > 1 {
            two_names += 1;
            let t = shown[&(*iri, *origin)];
            s.findings.push(Finding {
                check: "identifier with two names".into(),
                element: curie(iri),
                origin: *origin,
                source: t.source.clone(),
                status: status(t),
                detail: format!(
                    "named {}",
                    labels
                        .iter()
                        .map(|l| format!("'{l}'"))
                        .collect::<Vec<_>>()
                        .join(" and ")
                ),
            });
        }
    }

    let Some(rels) = releases else {
        s.metric("identifiers with two names", two_names);
        s.notes.push(
            "Source releases were not read (run without --offline, or add releases.toml), so only identifiers \
             with two names were checked."
                .into(),
        );
        return s.finish();
    };

    let mut by_release: BTreeMap<String, [u64; 6]> = BTreeMap::new();
    let mut unmatched_sources: BTreeMap<String, u64> = BTreeMap::new();
    for origin in [Origin::Statements, Origin::Candidates] {
        if !index.has(origin) {
            continue;
        }
        let o = origin_name(origin);
        let (mut checked, mut clean) = (0u64, 0u64);
        let mut counts = [0u64; 6];
        // Candidates the desk marked Out are not checked, as in the structure section.
        for t in index
            .terms
            .iter()
            .filter(|t| t.origin == origin && in_scope(index, t))
        {
            let Some(iri) = t.iri.as_deref() else {
                continue;
            };
            if BUILT_IN.iter().any(|ns| iri.starts_with(ns)) {
                continue;
            }
            let Some((rel, rt)) = rels.find(iri, t.source.as_deref()) else {
                if origin == Origin::Candidates {
                    *unmatched_sources
                        .entry(t.source.clone().unwrap_or_else(|| "(no source)".into()))
                        .or_default() += 1;
                }
                continue;
            };
            checked += 1;
            let problems = check(t, rel, rt);
            if problems.is_empty() {
                clean += 1;
            }
            let row = by_release.entry(rel.name.clone()).or_default();
            for (k, detail) in problems {
                counts[k] += 1;
                row[k] += 1;
                s.findings.push(Finding {
                    check: PROBLEMS[k].into(),
                    element: format!("{} ({})", t.label, curie(iri)),
                    origin,
                    source: t.source.clone().or_else(|| Some(rel.name.clone())),
                    status: status(t),
                    detail,
                });
            }
        }
        if checked == 0 {
            continue;
        }
        s.metric(format!("reused terms checked ({o})"), checked);
        s.metric(format!("reused terms matching their release ({o})"), clean);
        for (k, name) in PROBLEMS.iter().enumerate() {
            s.metric(format!("{name} ({o})"), counts[k]);
        }
        s.charts.push(Chart::Ratio {
            title: format!("Reused terms matching their release ({o})"),
            part: clean,
            whole: checked,
            direction: Direction::HigherIsBetter,
        });
    }
    s.metric("identifiers with two names", two_names);
    s.metric("source releases read", rels.releases.len());
    s.metric("source releases that could not be read", rels.failed.len());
    let differ: Vec<&Release> = rels
        .releases
        .iter()
        .filter(|r| !r.version_matches())
        .collect();
    s.metric(
        "releases that differ from the recorded version",
        differ.len(),
    );
    if !by_release.is_empty() {
        s.charts.push(Chart::Stacked {
            title: "Reuse problems by source release".into(),
            series: PROBLEMS.iter().map(|p| p.to_string()).collect(),
            rows: by_release
                .into_iter()
                .map(|(label, v)| Row {
                    label,
                    values: v.to_vec(),
                })
                .collect(),
        });
    }
    if !unmatched_sources.is_empty() {
        s.charts.push(crate::report::bars(
            "Desk terms with no release to check against, by source",
            unmatched_sources,
            0,
        ));
    }
    for r in &rels.releases {
        let ver = r.version.as_deref().unwrap_or("no version stated");
        let mut line = format!(
            "{}: {} terms read from {} (version: {ver})",
            r.name,
            crate::thousands(r.terms.len() as u64),
            r.urls.join(", ")
        );
        if !r.version_matches() {
            line.push_str(&format!(
                "; the project recorded {}, which could not be fetched, so this file was used instead",
                r.expected.as_deref().unwrap_or("")
            ));
        }
        s.notes.push(line);
    }
    for (name, e) in &rels.failed {
        s.notes.push(format!("{name}: could not be read ({e})"));
    }
    s.notes.push(
        "A term is checked against the release of the source it was taken from: a source in the project's \
         releases.toml, or for OBO identifiers the OBO release on the date the project recorded. Labels and \
         definitions are compared ignoring case, spacing and a final full stop. Whether a mapping declared \
         exact is really exact (for example a unit per kilogram of solvent used for seawater) needs a domain \
         expert and is not checked."
            .into(),
    );
    s.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_identifier_shown_as_a_name_is_no_name() {
        let mut t = crate::model::Term {
            label: "dcterms:title".into(),
            key: String::new(),
            curie: None,
            iri: Some("http://purl.org/dc/terms/title".into()),
            kind: String::new(),
            definitions: vec![],
            definition_sources: vec![],
            parents: vec![],
            parent_iris: vec![],
            labels: vec![],
            origin: Origin::Statements,
            source: None,
            release: None,
            group: None,
        };
        assert!(unnamed(&t));
        t.label = "title".into();
        assert!(unnamed(&t));
        t.label = "Title of the resource".into();
        assert!(!unnamed(&t));
    }

    #[test]
    fn desk_qualifiers_are_not_part_of_the_label() {
        assert_eq!(bare_label("Agent (DwC)"), "Agent");
        assert_eq!(bare_label("Language (DwC 2)"), "Language");
        assert_eq!(bare_label("depth"), "depth");
    }

    #[test]
    fn normalization_ignores_case_space_and_full_stop() {
        assert_eq!(norm("A  Thing ."), norm("a thing"));
        assert_ne!(norm("a thing"), norm("a things"));
    }
}
