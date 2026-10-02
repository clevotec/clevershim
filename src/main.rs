use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use clevershim::install;

#[derive(Parser)]
#[command(
    name = "clevershim",
    version,
    about = "Shim winget portable packages onto a stable PATH directory"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Copy CleverShim into place, append PATH, sync, and register the logon task.
    Setup,
    /// Remove the logon task, PATH entry, shims, and uninstall key for this scope.
    Uninstall,
    /// Refresh shims for packages that are installed in each writable scope.
    Sync,
    /// Rewrite a shim whose target moved, or remove it when nothing provides it.
    Repair {
        /// Shim command name, such as ffmpeg.
        #[arg(long)]
        name: Option<String>,
        /// Full path of the shim exe to repair.
        #[arg(long)]
        shim: Option<PathBuf>,
        /// Print the repair report and still apply it unless --dry-run is set.
        #[arg(long)]
        explain: bool,
        /// Print the repair report without changing the sidecar.
        #[arg(long)]
        dry_run: bool,
        /// Called by the shim stub. Success stays silent.
        #[arg(long)]
        from_shim: bool,
    },
    /// Show the shims in the user and machine bins.
    List,
    /// Register or remove the per-user logon sync task.
    Hook {
        #[command(subcommand)]
        action: HookCommand,
    },
}

#[derive(Subcommand)]
enum HookCommand {
    Install,
    Remove,
}

fn main() -> ExitCode {
    let raw: Vec<String> = env::args().skip(1).collect();
    if raw.iter().any(|arg| arg.eq_ignore_ascii_case("/install")) {
        return finish(install::setup());
    }
    if raw.iter().any(|arg| arg.eq_ignore_ascii_case("/uninstall")) {
        return finish(install::uninstall());
    }
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(err) => {
            err.print().ok();
            return if err.use_stderr() {
                ExitCode::from(2)
            } else {
                ExitCode::SUCCESS
            };
        }
    };
    match cli.command {
        Commands::Setup => finish(install::setup()),
        Commands::Uninstall => finish(install::uninstall()),
        Commands::Sync => finish(install::sync_command()),
        Commands::List => finish(install::list_command()),
        Commands::Hook { action } => match action {
            HookCommand::Install => finish(install::hook_install()),
            HookCommand::Remove => finish(install::hook_remove()),
        },
        Commands::Repair {
            name,
            shim,
            explain,
            dry_run,
            from_shim,
        } => match install::repair_command(
            name.as_deref(),
            shim.as_deref(),
            explain,
            dry_run,
            from_shim,
        ) {
            Ok(0) => ExitCode::SUCCESS,
            Ok(code) => ExitCode::from(code as u8),
            Err(err) => {
                eprintln!("clevershim: {err:#}");
                ExitCode::from(1)
            }
        },
    }
}

fn finish(result: anyhow::Result<()>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("clevershim: {err:#}");
            ExitCode::from(1)
        }
    }
}
