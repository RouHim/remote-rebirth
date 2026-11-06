use std::ffi::OsString;
use std::io::{self, Write};

fn main() {
    tracing_subscriber::fmt()
        .with_writer(io::stderr)
        .with_max_level(tracing::Level::INFO)
        .init();

    let args: Vec<OsString> = std::env::args_os().collect();
    let mut backend = remote_rebirth_core::HidBackend::new();
    let clock = remote_rebirth_core::SystemClock;
    let state_dir = remote_rebirth_core::state_dir();
    let mut stdout = io::stdout().lock();
    let mut stderr = io::stderr().lock();
    let mut runtime = remote_rebirth_core::Runtime {
        backend: &mut backend,
        clock: &clock,
        stdout: &mut stdout,
        stderr: &mut stderr,
        state_dir,
    };

    let exit = remote_rebirth_core::run(&args, &mut runtime);
    let _ = stdout.flush();
    let _ = stderr.flush();
    std::process::exit(exit.0 as i32);
}
