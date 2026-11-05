use std::ffi::OsString;
use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::backup;
use crate::detect;
use crate::error::{Error, Exit};
use crate::runtime::Runtime;

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
        out: PathBuf,
        #[arg(long)]
        overwrite: bool,
        #[arg(long)]
        json: bool,
    },
    Inspect {
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        print_schema: bool,
    },
    Compose {
        #[arg(long)]
        model: PathBuf,
        #[arg(long)]
        out: PathBuf,
        #[arg(long)]
        overwrite: bool,
        #[arg(long)]
        json: bool,
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

pub fn run(args: &[OsString], runtime: &mut Runtime<'_>) -> Exit {
    let cli = match Cli::try_parse_from(args.iter().cloned()) {
        Ok(cli) => cli,
        Err(error) => {
            let _ = write!(runtime.stderr, "{error}");
            return Exit(2);
        }
    };

    match dispatch(cli.command, runtime) {
        Ok(()) => Exit(0),
        Err(error) => {
            let _ = writeln!(runtime.stderr, "error: {error}");
            Exit(error.exit_code())
        }
    }
}

fn dispatch(command: Command, runtime: &mut Runtime<'_>) -> Result<(), Error> {
    let Runtime {
        backend,
        clock,
        stdout,
        stderr: _,
        state_dir: _,
    } = runtime;

    match command {
        Command::Detect { json } => detect::run(&mut **backend, json, &mut **stdout),
        Command::Backup {
            out,
            overwrite,
            json,
        } => backup::run(
            &mut **backend,
            &**clock,
            &out,
            overwrite,
            json,
            &mut **stdout,
        ),
        Command::Inspect { .. } => Err(Error::Internal("inspect is not implemented yet".into())),
        Command::Compose { .. } => Err(Error::Internal("compose is not implemented yet".into())),
        Command::Write { .. } => Err(Error::Internal("write is not implemented yet".into())),
        Command::Restore { .. } => Err(Error::Internal("restore is not implemented yet".into())),
    }
}
