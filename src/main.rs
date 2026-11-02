use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use clap::Parser;
use ontoreport::{config, corpus, embed};

/// Quality report for OWL ontologies built from the project record.
#[derive(Parser, Debug)]
#[command(name = "ontoreport", version, about)]
struct Cli {
    /// Project or review-desk data folder to report on.
    project: PathBuf,

    /// Where to write the HTML report; the JSON report is written next to it.
    #[arg(long, default_value = "report.html")]
    out: PathBuf,

    /// Write only the JSON report.
    #[arg(long)]
    json_only: bool,

    /// Domain-concept list for the corpus-fit section (JSON or text, one concept per line). Default:
    /// concepts.json or concepts.txt in the project folder or its corpus/ folder.
    #[arg(long, value_name = "FILE")]
    concepts: Option<PathBuf>,

    /// Configuration file to use instead of ~/.ontoreport/config.toml.
    #[arg(long, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Skip the embedding model: no coherence scores, and corpus fit without them.
    #[arg(long)]
    no_embeddings: bool,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let t0 = Instant::now();
    let mut index = match ontoreport::load(&cli.project) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("ontoreport: {e}");
            return ExitCode::from(2);
        }
    };
    if let Some(c) = &cli.concepts {
        match corpus::load_concepts(c) {
            Ok(d) => {
                index.inputs.push(c.display().to_string());
                index.domain = Some(d);
            }
            Err(e) => {
                eprintln!("ontoreport: {e}");
                return ExitCode::from(2);
            }
        }
    }
    let t_load = t0.elapsed();

    // The configuration file is created on first run, even when this run does not embed anything.
    let cfg = match config::load(cli.config.as_deref()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("ontoreport: {e}");
            return ExitCode::from(2);
        }
    };
    let vectors: Result<embed::Vectors, String> = if cli.no_embeddings {
        Err("Run with --no-embeddings.".into())
    } else {
        let texts = corpus::texts_needed(&index, cfg.text);
        if texts.is_empty() {
            Ok(embed::Vectors::default())
        } else {
            embed::vectors(&cfg, &texts, || embed::Bert::load(&cfg.model)).map_err(|e| {
                eprintln!("ontoreport: embeddings unavailable: {e}");
                format!("The embedding model could not be used: {e}")
            })
        }
    };
    let t_embed = t0.elapsed() - t_load;
    let emb = vectors
        .as_ref()
        .map(|v| (v, cfg.text))
        .map_err(|e| e.clone());
    let report = ontoreport::report_with(&index, &emb);
    let t_sections = t0.elapsed() - t_load - t_embed;

    let json_path = cli.out.with_extension("json");
    let json = serde_json::to_string_pretty(&report).expect("report serializes");
    if let Err(e) = fs::write(&json_path, json + "\n") {
        eprintln!("ontoreport: {}: {e}", json_path.display());
        return ExitCode::from(2);
    }
    if !cli.json_only {
        if let Err(e) = fs::write(&cli.out, ontoreport::html::render(&report)) {
            eprintln!("ontoreport: {}: {e}", cli.out.display());
            return ExitCode::from(2);
        }
    }

    for s in &report.sections {
        let state = if s.ran {
            format!("{} findings", s.findings.len())
        } else {
            "not run".into()
        };
        eprintln!("  {:<16} {state}", s.title);
    }
    eprintln!(
        "{} terms · read {:.0} ms · embeddings {:.0} ms · sections {:.0} ms · total {:.0} ms",
        index.terms.len(),
        t_load.as_secs_f64() * 1e3,
        t_embed.as_secs_f64() * 1e3,
        t_sections.as_secs_f64() * 1e3,
        t0.elapsed().as_secs_f64() * 1e3
    );
    if let Ok(v) = &vectors {
        if !v.by_text.is_empty() {
            eprintln!(
                "embeddings: {} · {} texts, {} from the cache · config {}",
                v.model,
                v.by_text.len(),
                v.cached,
                cfg.path.display()
            );
        }
    }
    if cli.json_only {
        eprintln!("wrote {}", json_path.display());
    } else {
        eprintln!("wrote {} and {}", cli.out.display(), json_path.display());
    }
    ExitCode::SUCCESS
}
