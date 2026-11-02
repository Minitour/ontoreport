use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;

/// Quality report for OWL ontologies built from the project record.
#[derive(Parser, Debug)]
#[command(name = "ontoreport", version, about)]
struct Cli {
    /// Project folder (or OWL/TTL file) to report on.
    project: Option<PathBuf>,

    /// Where to write the HTML report (the JSON report is written next to it).
    #[arg(long, default_value = "report.html")]
    out: PathBuf,
}

fn main() -> ExitCode {
    let _cli = Cli::parse();
    eprintln!("{}", ontoreport::placeholder_message());
    ExitCode::from(2)
}
