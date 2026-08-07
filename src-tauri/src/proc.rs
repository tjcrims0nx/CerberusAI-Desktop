//! Child-process helpers.
//!
//! Every child this app spawns (node, npm, git, ollama, llama-server,
//! powershell) is a console-subsystem binary. On Windows a GUI process that
//! spawns one gets a console window allocated for it, which flashes up over the
//! app — most visibly when installing or starting an MCP plugin. `.no_window()`
//! sets `CREATE_NO_WINDOW` so the child runs headless; stdio pipes are
//! unaffected. It is a no-op on other platforms.

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub trait NoWindow {
    /// Suppress the console window Windows would otherwise allocate for this
    /// child process.
    fn no_window(&mut self) -> &mut Self;
}

impl NoWindow for std::process::Command {
    fn no_window(&mut self) -> &mut Self {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            self.creation_flags(CREATE_NO_WINDOW);
        }
        self
    }
}

impl NoWindow for tokio::process::Command {
    fn no_window(&mut self) -> &mut Self {
        // `tokio::process::Command` exposes `creation_flags` inherently on
        // Windows, so no `CommandExt` import is needed here.
        #[cfg(windows)]
        self.creation_flags(CREATE_NO_WINDOW);
        self
    }
}
