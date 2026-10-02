use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;

#[derive(Parser)]
#[command(
    name = "clevershim-scan",
    about = "Build catalog/packages.yaml from the public WinGet source index"
)]
struct Args {
    /// Catalog file to write. Collisions are written beside it.
    #[arg(long, default_value = "catalog/packages.yaml")]
    out: PathBuf,
    /// Overrides applied after the portable-command heuristic.
    #[arg(long, default_value = "catalog/overrides.yaml")]
    overrides: PathBuf,
}

fn main() -> ExitCode {
    let args = Args::parse();
    match clevershim::scan_index::run_scan(&args.out, &args.overrides) {
        Ok(_) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("clevershim-scan: {err:#}");
            ExitCode::from(1)
        }
    }
}
