use std::ffi::OsString;
use std::io::{IsTerminal, Write};
use std::path::PathBuf;

use clap::{Parser, Subcommand, error::ErrorKind};

use crate::backup;
use crate::compose;
use crate::detect;
use crate::error::{Error, Exit};
use crate::progress::Progress;
use crate::restore;
use crate::runtime::Runtime;
use crate::write;

#[derive(Debug, Parser)]
#[command(
    name = "remote-rebirth",
    version,
    about = "Offline configuration tool for the Logitech Harmony 300"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Detect {
        #[arg(long)]
        json: bool,
    },
    Backup {
        #[arg(long)]
        out: Option<PathBuf>,
        #[arg(long)]
        overwrite: bool,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        print_schema: bool,
    },
    Inspect {
        #[arg(long)]
        file: Option<PathBuf>,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        print_schema: bool,
    },
    Compose {
        #[arg(long)]
        model: Option<PathBuf>,
        #[arg(long)]
        out: Option<PathBuf>,
        #[arg(long)]
        overwrite: bool,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        print_schema: bool,
        #[arg(long)]
        print_resolved_schema: bool,
    },
    Write {
        #[arg(long)]
        container: PathBuf,
        #[arg(long)]
        backup: PathBuf,
        #[arg(long)]
        commit: bool,
        #[arg(long)]
        no_reboot: bool,
        #[arg(long)]
        json: bool,
    },
    Restore {
        #[arg(long)]
        backup: PathBuf,
        #[arg(long)]
        commit: bool,
        #[arg(long)]
        no_reboot: bool,
        #[arg(long)]
        json: bool,
    },
}

impl Command {
    fn name(&self) -> &'static str {
        match self {
            Self::Detect { .. } => "detect",
            Self::Backup { .. } => "backup",
            Self::Inspect { .. } => "inspect",
            Self::Compose { .. } => "compose",
            Self::Write { .. } => "write",
            Self::Restore { .. } => "restore",
        }
    }
}

pub fn run(args: &[OsString], runtime: &mut Runtime<'_>) -> Exit {
    let cli = match Cli::try_parse_from(args.iter().cloned()) {
        Ok(cli) => cli,
        Err(error) => return report_clap_error(error, runtime),
    };

    let span = tracing::info_span!("command", name = cli.command.name());
    let _entered = span.enter();
    tracing::debug!(argument_count = args.len(), "dispatching");

    match dispatch(cli.command, runtime) {
        Ok(()) => Exit(0),
        Err(error) => {
            let exit = error.exit_code();
            tracing::error!(%error, code = exit, "command failed");
            let _ = writeln!(runtime.stderr, "error: {error}");
            Exit(exit)
        }
    }
}

/// Help and version are successful output on stdout; every other clap error is a usage error.
fn report_clap_error(error: clap::Error, runtime: &mut Runtime<'_>) -> Exit {
    match error.kind() {
        ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => {
            let _ = write!(runtime.stdout, "{error}");
            Exit(0)
        }
        _ => {
            let _ = write!(runtime.stderr, "{error}");
            Exit(2)
        }
    }
}

fn dispatch(command: Command, runtime: &mut Runtime<'_>) -> Result<(), Error> {
    let Runtime {
        backend,
        clock,
        stdout,
        stderr: _,
        state_dir,
    } = runtime;

    match command {
        Command::Detect { json } => detect::run(&mut **backend, json, &mut **stdout),
        Command::Backup {
            out,
            overwrite,
            json,
            print_schema,
        } => {
            if print_schema {
                stdout.write_all(backup::SCHEMA.as_bytes())?;
                return Ok(());
            }
            let out = out.ok_or_else(|| Error::Usage("backup requires --out <path>".into()))?;
            backup::run(
                &mut **backend,
                &**clock,
                &out,
                overwrite,
                json,
                &mut **stdout,
            )
        }
        Command::Inspect {
            file,
            json,
            print_schema,
        } => crate::inspect::run(file.as_deref(), json, print_schema, &mut **stdout),
        Command::Compose {
            model,
            out,
            overwrite,
            json,
            print_schema,
            print_resolved_schema,
        } => {
            if print_schema {
                stdout.write_all(compose::MODEL_SCHEMA.as_bytes())?;
            }
            if print_resolved_schema {
                stdout.write_all(compose::RESOLVED_SCHEMA.as_bytes())?;
            }
            if print_schema || print_resolved_schema {
                return Ok(());
            }
            let model = model
                .ok_or_else(|| Error::Usage("compose requires --model <model.json>".into()))?;
            let out =
                out.ok_or_else(|| Error::Usage("compose requires --out <container.bin>".into()))?;
            compose::run(&**clock, &model, &out, overwrite, json, &mut **stdout)
        }
        Command::Write {
            container,
            backup,
            commit,
            no_reboot,
            json,
        } => {
            let mut progress = progress_sink();
            write::run(
                &mut **backend,
                &**clock,
                crate::write::WriteRequest {
                    state_dir,
                    container_path: &container,
                    backup_path: &backup,
                    commit,
                    no_reboot,
                },
                json,
                &mut **stdout,
                &mut progress,
            )
        }
        Command::Restore {
            backup,
            commit,
            no_reboot,
            json,
        } => {
            let mut progress = progress_sink();
            restore::run(
                &mut **backend,
                &**clock,
                crate::restore::RestoreRequest {
                    state_dir,
                    backup_path: &backup,
                    commit,
                    no_reboot,
                },
                json,
                &mut **stdout,
                &mut progress,
            )
        }
    }
}

/// Render Write and Restore progress to stderr, and only when stderr is a terminal, so a piped
/// transcript stays clean.
fn progress_sink() -> impl FnMut(Progress) {
    let interactive = std::io::stderr().is_terminal();
    move |event| {
        if !interactive {
            return;
        }
        match event {
            Progress::Step(step) => eprintln!("{step}"),
            Progress::Written { bytes, total } => eprintln!("{bytes}/{total} bytes"),
        }
    }
}
