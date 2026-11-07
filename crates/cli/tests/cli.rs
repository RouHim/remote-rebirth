//! The real binary, not the in-process seam: argv handling, stream separation and exit codes.

use std::process::Command;

fn binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_remote-rebirth"))
}

#[test]
fn version_prints_to_stdout_and_exits_zero() {
    let output = binary().arg("--version").output().expect("run --version");

    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("remote-rebirth"), "{stdout}");
    assert!(stdout.contains(env!("CARGO_PKG_VERSION")), "{stdout}");
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn help_prints_the_command_list_and_exits_zero() {
    let output = binary().arg("--help").output().expect("run --help");

    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    for command in ["detect", "backup", "inspect", "compose", "write", "restore"] {
        assert!(stdout.contains(command), "{command} missing from {stdout}");
    }
}

#[test]
fn a_missing_subcommand_is_a_usage_error_on_stderr() {
    let output = binary().output().expect("run without arguments");

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(
        output.stdout.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(!output.stderr.is_empty(), "a usage error must be explained");
}

#[test]
fn an_unknown_subcommand_is_a_usage_error() {
    let output = binary().arg("teleport").output().expect("run teleport");

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(
        output.stdout.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("teleport"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn print_schema_needs_no_remote_and_writes_json_to_stdout() {
    let output = binary()
        .args(["backup", "--print-schema"])
        .output()
        .expect("run backup --print-schema");

    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.trim_start().starts_with('{'), "{stdout}");
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
