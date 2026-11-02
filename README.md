# ontoreport

[![CI](https://github.com/Minitour/ontoreport/actions/workflows/ci.yml/badge.svg)](https://github.com/Minitour/ontoreport/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://opensource.org/licenses/MIT)

A quality report for OWL ontologies that reads the **project record** as well as the ontology:
whether every term traces to a requirement, every definition to a cited source, every reused term
to its source release, and how much of the ontology has been reviewed. One native binary per
platform, written in Rust, distributed on npm and PyPI.

> **Status: early development.** The published 0.0.1 is a placeholder; build from source for the
> current version. Reuse fidelity, requirements coverage from a requirements file, review decisions
> and corpus fit come next.

## Usage

```bash
ontoreport <project-folder | ontology-file> --out report.html
```

Writes `report.html` (a dashboard for people) and `report.json` (the same content for agents). The
JSON is deterministic: the same inputs give the same bytes, so reports can be diffed and kept in
version control. Run time goes to stderr; set `ONTOREPORT_TIMING=1` for the time of each check.

Every metric, KPI and chart has a (?) icon that explains in plain words what it is and how it is
measured; the JSON carries the same text in each metric's `help` field.

## Inputs

All inputs are optional; a section whose input is missing is marked "not run".

| Input | Where it is looked for |
|---|---|
| Ontology files: RDF/XML (`.owl`, `.rdf`), OWL functional syntax (`.ofn`), Turtle (`.ttl`), N-Triples (`.nt`) | the folder, `ontology/`, `ontologies/`, `src/ontology/`, or a single file |
| Review-desk data: `g0.json` (statements of an ontology file), `groups.json` (candidate terms), `check.json` (the desk's self-check) | the folder or `data/` |
| Recorded decisions: `decisions.json` | the folder or `qc/` |

A file that cannot be read is reported as a finding and the rest of the report goes ahead. Labels,
definitions and definition sources are read from the RDF directly, so they count even when a file
does not declare its annotation properties.

`decisions.json` records decisions about findings, so the report shows the decision instead of
raising the finding again:

```json
{
  "pitfalls": [
    {"id": "P11", "elements": ["*"], "decision": "accepted",
     "reason": "domain and range left open, as in RO", "ref": "notebook E33"}
  ]
}
```

## Sections

| Section | What it reports |
|---|---|
| Consistency and pitfalls | OWL 2 EL consistency and unsatisfiable classes (whelk); the 31 OOPS!-style pitfalls of OWL-MCP, each with its recorded decision; imports not loaded |
| Structure | Classes without a path to the top class: BFO's *entity* when the project uses BFO, otherwise `owl:Thing` |
| Provenance | Terms without a definition; definitions citing a weak source; definitions with their own source annotation |
| Traceability | What each kept group on a review desk says it serves (competency questions, use cases) |
| Review status | Candidate terms by source and status, with a drill-down by source, category and group |

Planned: reuse fidelity against source releases, requirements coverage, review decisions from a
desk export, and corpus fit (domain coverage and concept-family coherence, with all-MiniLM-L6-v2).

## Install

```bash
npx ontoreport --version
```

```bash
pipx run ontoreport --version
```

Or build from source:

```bash
cargo install --git https://github.com/Minitour/ontoreport
```

## Releasing

Releases are published by `.github/workflows/release.yml` from a version tag, with no stored
tokens: npm through trusted publishing (GitHub OIDC) and PyPI through a trusted publisher. The
first npm version has to be published once by hand; see `scripts/npm-bootstrap.sh`.

## License

MIT
