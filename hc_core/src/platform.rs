use std::path::PathBuf;

/// Returns the platform-specific data directory joined with "hcime".
///
/// On Linux this resolves to `$XDG_DATA_HOME/hcime` or `~/.local/share/hcime`.
pub fn data_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|p| p.join("hcime"))
}

/// Returns the platform-specific config directory joined with "hcime".
///
/// On Linux this resolves to `$XDG_CONFIG_HOME/hcime` or `~/.config/hcime`.
pub fn config_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|p| p.join("hcime"))
}

/// Returns the platform-specific state directory joined with "hcime".
///
/// On Linux this resolves to `$XDG_STATE_HOME/hcime` or `~/.local/state/hcime`.
///
/// Only the XDG platforms define a state directory, so `dirs::state_dir()` is
/// `None` on macOS and Windows. Those platforms fall back to the data directory
/// (`~/Library/Application Support/hcime`, `%APPDATA%\hcime`), which is where
/// they conventionally keep this kind of file. Without the fallback, callers
/// degrade to a relative path and write user state into the host app's working
/// directory. Linux is unaffected: `dirs::state_dir()` is always `Some` there.
pub fn state_dir() -> Option<PathBuf> {
    dirs::state_dir()
        .or_else(dirs::data_dir)
        .map(|p| p.join("hcime"))
}

/// Returns a priority-ordered list of candidate dictionary search directories.
///
/// The search order is:
/// 1. The parent directory of the `HC_IME_NOM_DICT` environment variable (if set).
/// 2. The platform data directory (`data_dir()`) + "data".
/// 3. System-wide Fcitx5 addon data directories on Linux.
pub fn dictionary_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    if let Ok(val) = std::env::var("HC_IME_NOM_DICT") {
        if let Some(parent) = std::path::Path::new(&val).parent() {
            paths.push(parent.to_path_buf());
        }
    }

    if let Some(data) = data_dir() {
        paths.push(data.join("data"));
    }

    paths.push(PathBuf::from("/usr/share/fcitx5/addon/hcime/data"));
    paths.push(PathBuf::from("/usr/local/share/fcitx5/addon/hcime/data"));

    paths
}
