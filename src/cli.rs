use std::ffi::OsString;
use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::backup;
use crate::compose;
use crate::detect;
use crate::error::{Error, Exit};
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
        out: PathBuf,
        #[arg(long)]
        overwrite: bool,
        #[arg(long)]
        json: bool,
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
        state_dir,
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
        } => compose::run(&**clock, &model, &out, overwrite, json, &mut **stdout),
        Command::Write {
            container,
            backup,
            commit,
            no_reboot,
            json,
        } => write::run(
            &mut **backend,
            &**clock,
            state_dir,
            &container,
            &backup,
            commit,
            no_reboot,
            json,
            &mut **stdout,
        ),
        Command::Restore {
            backup,
            commit,
            no_reboot,
            json,
        } => restore::run(
            &mut **backend,
            &**clock,
            state_dir,
            &backup,
            commit,
            no_reboot,
            json,
            &mut **stdout,
        ),
    }
}
