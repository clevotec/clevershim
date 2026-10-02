#![allow(dead_code)]

//! Original CleverShim stub. It reads a sidecar, asks the manager to repair once
//! when the target is missing, and starts the process the manager recorded.
//! This is not derived from Chocolatey shimgen.

#[path = "../sidecar_format.rs"]
mod sidecar_format;

use std::env;
use std::io::{self, Write};
use std::path::Path;
use std::process::{Command, Stdio};

use sidecar_format::{launch_plan, sidecar_path_for_exe, LaunchPlan, Sidecar};

fn main() {
    let exe = match env::current_exe() {
        Ok(exe) => exe,
        Err(err) => fail(&format!("could not locate this shim: {err}")),
    };
    let mut forwarded = Vec::new();
    let mut noop = false;
    let mut test = false;
    for arg in env::args().skip(1) {
        if arg == "--clevershim-noop" {
            noop = true;
        } else if arg == "--clevershim-test" {
            test = true;
        } else {
            forwarded.push(arg);
        }
    }
    let sidecar_path = sidecar_path_for_exe(&exe);
    let sidecar = match sidecar_format::load(&sidecar_path) {
        Ok(sidecar) => sidecar,
        Err(err) => fail(&format!("could not read {}: {err}", sidecar_path.display())),
    };
    if noop || test {
        if sidecar.manager.is_empty() {
            println!("{}", mapping(&sidecar));
            std::process::exit(0);
        }
        let mut command = Command::new(&sidecar.manager);
        command
            .arg("repair")
            .arg("--name")
            .arg(&sidecar.command)
            .arg("--shim")
            .arg(&exe)
            .arg("--explain");
        if noop && !test {
            command.arg("--dry-run");
        }
        let status = command.status();
        match status {
            Ok(status) => std::process::exit(status.code().unwrap_or(1)),
            Err(err) => fail(&format!("could not run manager: {err}")),
        }
    }

    let mut sidecar = sidecar;
    let mut repaired = false;
    if matches!(
        launch_plan(target_exists(&sidecar), repaired),
        LaunchPlan::RepairOnce
    ) {
        repaired = true;
        match run_repair(&sidecar, &exe) {
            RepairOutcome::Ready(next) => sidecar = next,
            RepairOutcome::Failed { message, code } => {
                let _ = writeln!(io::stderr(), "{message}");
                std::process::exit(code);
            }
        }
    }
    if matches!(
        launch_plan(target_exists(&sidecar), repaired),
        LaunchPlan::GiveUp
    ) {
        let _ = writeln!(
            io::stderr(),
            "{} is still missing after repair",
            sidecar.command
        );
        std::process::exit(1);
    }
    let code = launch(&sidecar, &forwarded);
    std::process::exit(code);
}

enum RepairOutcome {
    Ready(Sidecar),
    Failed { message: String, code: i32 },
}

fn run_repair(sidecar: &Sidecar, exe: &Path) -> RepairOutcome {
    if sidecar.manager.is_empty() {
        return RepairOutcome::Failed {
            message: format!("{} has no clevershim manager path", sidecar.command),
            code: 1,
        };
    }
    let output = Command::new(&sidecar.manager)
        .arg("repair")
        .arg("--name")
        .arg(&sidecar.command)
        .arg("--shim")
        .arg(exe)
        .arg("--from-shim")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();
    let output = match output {
        Ok(output) => output,
        Err(err) => {
            return RepairOutcome::Failed {
                message: format!("could not run clevershim repair: {err}"),
                code: 1,
            }
        }
    };
    let code = output.status.code().unwrap_or(1);
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let detail = if !stderr.is_empty() { stderr } else { stdout };
    if code == 3 {
        schedule_delete(exe);
        return RepairOutcome::Failed {
            message: if detail.is_empty() {
                format!("{} is no longer installed", sidecar.command)
            } else {
                detail
            },
            code: 3,
        };
    }
    let reloaded = sidecar_format::load(&sidecar_path_for_exe(exe));
    match reloaded {
        Ok(next) if target_exists(&next) && code == 0 => RepairOutcome::Ready(next),
        Ok(_) | Err(_) => RepairOutcome::Failed {
            message: if detail.is_empty() {
                format!("{} could not be repaired", sidecar.command)
            } else {
                detail
            },
            code: if code == 0 { 1 } else { code },
        },
    }
}

fn launch(sidecar: &Sidecar, args: &[String]) -> i32 {
    let (program, mut program_args) = if sidecar.runner.is_empty() {
        (sidecar.target.clone(), Vec::new())
    } else {
        (sidecar.runner.clone(), sidecar.runner_args.clone())
    };
    program_args.extend(args.iter().cloned());
    let mut command = Command::new(&program);
    command.args(&program_args);
    if !sidecar.working_directory.is_empty() {
        command.current_dir(&sidecar.working_directory);
    }
    if sidecar.path_prefix {
        let prefix = if sidecar.working_directory.is_empty() {
            Path::new(&sidecar.target)
                .parent()
                .map(|path| path.to_path_buf())
                .unwrap_or_else(|| Path::new(".").to_path_buf())
        } else {
            Path::new(&sidecar.working_directory).to_path_buf()
        };
        let mut joined = prefix.into_os_string();
        if let Some(existing) = env::var_os("PATH") {
            joined.push(if cfg!(windows) { ";" } else { ":" });
            joined.push(existing);
        }
        command.env("PATH", &joined);
        if cfg!(windows) {
            command.env("Path", &joined);
        }
    }
    match command.status() {
        Ok(status) => status.code().unwrap_or(1),
        Err(err) => {
            let _ = writeln!(io::stderr(), "could not start {program}: {err}");
            1
        }
    }
}

fn target_exists(sidecar: &Sidecar) -> bool {
    !sidecar.target.is_empty() && Path::new(&sidecar.target).is_file()
}

fn mapping(sidecar: &Sidecar) -> String {
    format!(
        "shim: {}\nscope: {}\npackage: {}\ntarget: {}\nrunner: {}\nworking_directory: {}\npath_prefix: {}\nmanager: {}",
        sidecar.command,
        sidecar.scope,
        sidecar.package_id,
        sidecar.target,
        sidecar.runner,
        sidecar.working_directory,
        sidecar.path_prefix,
        sidecar.manager
    )
}

fn schedule_delete(exe: &Path) {
    #[cfg(windows)]
    {
        let script = format!("ping -n 3 127.0.0.1 >nul & del /f /q \"{}\"", exe.display());
        let _ = Command::new("cmd").args(["/c", &script]).spawn();
    }
    #[cfg(not(windows))]
    {
        let _ = exe;
    }
}

fn fail(message: &str) -> ! {
    let _ = writeln!(io::stderr(), "{message}");
    std::process::exit(1);
}
