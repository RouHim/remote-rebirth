use std::path::{Path, PathBuf};

/// The sibling sidecar path a Backup or Container's JSON metadata is written to.
pub fn sidecar_path(path: &Path) -> PathBuf {
    with_suffix(path, ".json")
}

/// `path` with `suffix` appended to its whole file name, including any extension.
pub fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

pub fn state_dir() -> PathBuf {
    if let Some(xdg) = std::env::var_os("XDG_STATE_HOME")
        && !xdg.is_empty()
    {
        return PathBuf::from(xdg).join("remote-rebirth");
    }
    if let Some(home) = std::env::var_os("HOME")
        && !home.is_empty()
    {
        return PathBuf::from(home).join(".local/state/remote-rebirth");
    }
    PathBuf::from(".remote-rebirth-state")
}
