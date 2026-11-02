# ontoreport

[![CI](https://github.com/Minitour/ontoreport/actions/workflows/ci.yml/badge.svg)](https://github.com/Minitour/ontoreport/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://opensource.org/licenses/MIT)

A quality report for OWL ontologies that reads the **project record** as well as the ontology:
whether every term traces to a requirement, every definition to a cited source, every reused term
to its source release, and how much of the ontology has been reviewed. One native binary per
platform, written in Rust, distributed on npm and PyPI.

> **Status: early development.** The published 0.0.1 is a placeholder. The current source reads
> review-desk data (statement views of ontology files, candidate terms, the desk's own checks) and
> reports Structure (classes without a path to the top class), Provenance (missing definitions,
> weak sources), Traceability (what each group says it serves) and Review status. Reading OWL files
> directly, consistency, pitfalls and reuse fidelity come next.

## Usage

```bash
ontoreport <project-or-desk-folder> --out report.html
```

Writes `report.html` and `report.json` (same content). The JSON is deterministic: the same inputs give
the same bytes, so reports can be diffed and kept in version control. Run time goes to stderr.

## Sections

| Section | What it reports |
|---|---|
| Structure | Consistency, unsatisfiable classes, pitfalls with their recorded decisions, classes without a path to the top class |
| Reuse fidelity | Reused identifiers that are missing, deprecated or obsolete; labels and definitions that differ from the source release; exact matches that are not exact |
| Traceability | Terms with no competency question, use case or goal; planned-answer terms missing from the ontology |
| Provenance | Definitions without a cited source or locator; sources missing from the bibliography; identifiers that do not resolve |
| Requirements coverage | Competency questions per module; modules without questions; questions answered by SPARQL |
| Review status | Statements approved, debatable, removed or not yet reviewed |
| Corpus fit (optional) | Domain coverage, relevance and concept-family coherence against a domain corpus |

The report is deterministic: the same project gives the same findings. It is written twice, as a
self-contained HTML page for people and as JSON for agents.

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
