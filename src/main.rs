use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use clap::Parser;

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
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let t0 = Instant::now();
    let index = match ontoreport::load(&cli.project) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("ontoreport: {e}");
            return ExitCode::from(2);
        }
    };
    let t_load = t0.elapsed();
    let report = ontoreport::report(&index);
    let t_sections = t0.elapsed() - t_load;

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
        "{} terms · read {:.0} ms · sections {:.0} ms · total {:.0} ms",
        index.terms.len(),
        t_load.as_secs_f64() * 1e3,
        t_sections.as_secs_f64() * 1e3,
        t0.elapsed().as_secs_f64() * 1e3
    );
    if cli.json_only {
        eprintln!("wrote {}", json_path.display());
    } else {
        eprintln!("wrote {} and {}", cli.out.display(), json_path.display());
    }
    ExitCode::SUCCESS
}
