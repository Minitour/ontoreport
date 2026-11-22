---
name: ontoreport
description: Assess the quality of an OWL ontology or an ontology project with the `ontoreport` CLI (npm/PyPI package ontoreport) and act on what it finds. One command writes an HTML dashboard and a deterministic JSON report covering OWL 2 EL consistency and unsatisfiable classes, the OOPS! pitfalls, classes with no path to the top class, definitions and their sources, reused terms checked against their source releases (obsolete, missing, renamed, redefined), review-desk traceability and status, semantic coherence of concept families (CSS, PSS, PDA), and coverage and relevance against a domain corpus. Use this whenever the user wants to evaluate, audit, quality-check, validate or review an ontology (.owl, .ttl, .ofn, .rdf), an AODK project or review desk, check reused OBO/ENVO/OBI/IAO terms against their releases, find obsolete or deprecated terms, measure domain coverage or concept-family coherence, run OOPS!-style pitfall checks, or produce a shareable ontology quality report — even if they never say "ontoreport".
---

# Ontology quality reports with ontoreport

`ontoreport` reads an ontology project — the OWL files, and when present a review desk's data, recorded
decisions, a domain-concept list and a list of source releases — and writes two files: `report.html`, a
dashboard for people, and `report.json`, the same content for agents. The JSON is deterministic, so two
reports can be diffed to see what a change fixed.

Each check is computed, not judged: the report says *what is the case* (this class is unsatisfiable, this
reused term is obsolete in its source release). Whether a finding is a real problem, an accepted design
choice or a false alarm is the part that needs you and the user. The usual job is: *run the report, read
the JSON, triage the findings, fix or record decisions, re-run.*

## 1. Get the tool

```bash
npx -y ontoreport --version      # npm, no install
pipx run ontoreport --version    # or PyPI (uvx ontoreport also works)
```

Version 0.0.1 was a placeholder without the checks; use 1.0.0 or later. To build from source:
`cargo install --git https://github.com/Minitour/ontoreport`. Every flag is listed in
`references/cli.md`.

## 2. Lay out the inputs

Point ontoreport at the **project folder**, not a single file, so it finds everything it can use (a
single `.owl` file also works but then only that file is read):

| Put in the folder (or the subfolder shown) | Enables |
|---|---|
| ontology files: `.owl` `.rdf` `.ttl` `.ofn` `.nt` (also `ontology/`, `src/ontology/`) | consistency, pitfalls, structure, provenance, coherence |
| review-desk data `g0.json`, `groups.json`, `check.json` (or `data/`) | traceability, review status, reuse of desk candidates |
| `decisions.json` (or `qc/`) | findings shown as decided (accepted, fixed) instead of open |
| `releases.toml` | reuse fidelity for sources that are not OBO ontologies |
| `concepts.json` / `concepts.txt` (or `corpus/`, or `--concepts FILE`) | corpus fit |

Missing inputs are not errors: a section with nothing to read is marked "not run" with the reason.

**Source releases.** For OBO ontologies (ENVO, OBI, IAO, RO, BFO, COB …) ontoreport fetches the release
itself — but only when the project records which release it used (a desk's release text such as
`ENVO 2026-10-23`). For an ontology project without a desk, or for W3C, TDWG, NERC or other
vocabularies, write a `releases.toml`; the format, including CSV term tables, is in `references/cli.md`.
Pin versions (a dated release, a commit) rather than "latest", so the report can be reproduced.

## 3. Run it

```bash
ontoreport path/to/project --out report.html
```

The first run creates `~/.ontoreport/config.toml`, downloads the sentence-embedding model (about 91 MB,
for coherence and corpus fit) and the source releases, and caches them; later runs are fast and
offline-capable. Tell the user before a first run that it downloads these. Useful flags:
`--offline` (download nothing), `--no-embeddings` (skip the model), `--json-only`.
Exit code 0 means a report was written; 2 means an input or configuration error, printed on stderr.

## 4. Read the JSON, not the HTML

`report.json` has `overview.kpis` (headline numbers), and `sections[]`, each with `id`, `ran`, `notes`,
`metrics[]` (name, value, plain-language `help`) and `findings[]` (`check`, `element`, `origin`,
`source`, `status`, `detail`). Section ids: `logic`, `structure`, `provenance`, `reuse`,
`traceability`, `review`, `coherence`, `corpus`. For example:

```bash
jq -r '.sections[] | "\(.id)\t\(.ran)\t\(.findings|length)"' report.json
jq -r '.sections[] | select(.id=="logic") | .findings[] | "\(.check) | \(.element) | \(.status)"' report.json
jq -r '[.sections[].findings[].check] | group_by(.) | map("\(length)\t\(.[0])") | .[]' report.json
```

Always read each section's `notes`: they say what was read (releases and their versions, the model) and
what was not, which changes how a zero should be read.

## 5. Triage the findings

Work from the most serious down, and keep three outcomes apart: **fix**, **record a decision**, or
**leave for an expert**. `references/findings.md` explains every check, its usual cause, the fix, and how
often it is a false alarm. In short:

- **Unsatisfiable classes** and **inconsistency** are errors: find the axioms that clash (often a class
  under two disjoint parents) and fix them.
- **Pitfalls** are conventions. Many are deliberate (P11, domain and range left open, follows RO).
  Record those in `decisions.json` instead of changing the ontology.
- **No path to the top class**, **no definition**, **weak definition source** are gaps to fill.
- **Reuse fidelity** findings compare the project with its sources: an obsolete term should be replaced
  by the one the finding names; a label or definition that differs may be a deliberate edit (then record
  it) or drift (then update it). Check the source release, quoted in the finding, before changing anything.
- **Coherence** flags ("child may not belong under its parent") and **corpus fit** gaps are statistical
  signals for review, validated against experts at about 85–89% agreement. Never present them as errors
  and never move classes on their strength alone.

Do not edit the ontology to make the report quiet. Change it only for problems the user agrees are
problems, and say what each change does.

## 6. Record decisions

`decisions.json` in the project folder (or `qc/`) turns a finding from open into decided:

```json
{
  "pitfalls": [
    {"id": "P11", "elements": ["*"], "decision": "accepted",
     "reason": "Domain and range left open, as in RO", "ref": "design notes 2026-11"}
  ],
  "findings": []
}
```

`elements` lists the affected elements as the report names them (`"part of (partOf)"`), or `["*"]` for
all. Always give the reason in the user's words and a `ref` to where it was decided.

## 7. Re-run and hand over

Re-run after fixes and compare the two JSON files (`diff`, or count findings per check before and after).
Give the user:
- the paths of `report.html` and `report.json`;
- the headline KPIs and the findings by section, with the most serious first;
- what you changed, what you recorded as decisions, and what needs a domain expert;
- anything a section could not check and why (no release, no concept list, model not downloaded).

The HTML report is self-contained: it can be opened locally, attached, or published as it is.
