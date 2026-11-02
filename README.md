# ontoreport

[![CI](https://github.com/Minitour/ontoreport/actions/workflows/ci.yml/badge.svg)](https://github.com/Minitour/ontoreport/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://opensource.org/licenses/MIT)

A quality report for OWL ontologies that reads the **project record** as well as the ontology:
whether every term traces to a requirement, every definition to a cited source, every reused term
to its source release, and how much of the ontology has been reviewed. One native binary per
platform, written in Rust, distributed on npm and PyPI.

> **Status: placeholder release.** Versions 0.0.x reserve the package names. The report is under
> development; running it prints a notice and exits with code 2.

## Planned sections

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
