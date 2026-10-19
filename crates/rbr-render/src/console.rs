//! Windows: the launcher and the viewer are GUI programs (no console window), so what they print to stderr goes to a
//! log file in the player's folder, and the console programs they start (`rbr-standalone`, PowerShell's dialogs) open
//! no console window. Nothing changes elsewhere.

use std::process::Command;

/// The log file's name in the player's folder (Windows).
pub const LOG_FILE: &str = "rerbr.log";

/// `command` without a console window on Windows (`CREATE_NO_WINDOW`); its pipes and inherited handles unchanged.
pub fn no_window(command: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

/// Points stderr at `path`, rewritten, when the program has none (a GUI program started from Explorer or a console):
/// the programs it starts inherit it, so the viewer's and `rbr-standalone`'s lines land there too. Started with a
/// stderr (Git Bash, a redirect), it keeps that.
#[cfg(windows)]
pub fn log_to(path: &std::path::Path) {
    use std::os::windows::io::IntoRawHandle;
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::System::Console::{GetStdHandle, SetStdHandle, STD_ERROR_HANDLE};
    // SAFETY: GetStdHandle only reads the process's handle table.
    let current = unsafe { GetStdHandle(STD_ERROR_HANDLE) };
    if !current.is_null() && current != INVALID_HANDLE_VALUE {
        return;
    }
    let Ok(file) = std::fs::File::create(path) else { return };
    // SAFETY: the handle is a file opened for writing, left open for the rest of the process.
    unsafe { SetStdHandle(STD_ERROR_HANDLE, file.into_raw_handle()) };
}
