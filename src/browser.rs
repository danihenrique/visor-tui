//! Opening a URL in the default browser.

use std::process::{Command, Stdio};

/// Opens `url` in the default browser.
/// Returns `Ok(())` when the process was spawned successfully, or an error
/// message describing why it could not be launched.
pub fn open_in_browser(url: &str) -> Result<(), String> {
    let mut command = if cfg!(target_os = "macos") {
        let mut cmd = Command::new("open");
        cmd.arg(url);
        cmd
    } else if cfg!(windows) {
        // `start` would split the URL at `&`; use rundll32 instead.
        let mut cmd = Command::new("rundll32");
        cmd.args(["url.dll,FileProtocolHandler", url]);
        cmd
    } else {
        let mut cmd = Command::new("xdg-open");
        cmd.arg(url);
        cmd
    };

    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Não foi possível abrir o navegador: {e}"))
}