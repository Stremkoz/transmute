//! Running short-lived tool commands (version and capability probes) with a
//! timeout, so a misbehaving tool can never hang discovery.
//!
//! Long-running conversions use the engine's supervised runner instead (M5).

use std::ffi::OsStr;
use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedOutput {
    /// `None` if the process was terminated by a signal.
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl CapturedOutput {
    /// stdout followed by stderr: tools disagree on where they print versions.
    pub fn combined(&self) -> String {
        format!("{}{}", self.stdout, self.stderr)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error("could not start: {0}")]
    Spawn(#[source] std::io::Error),
    #[error("did not finish within {0:?}; it was stopped")]
    Timeout(Duration),
    #[error("failed while waiting: {0}")]
    Wait(#[source] std::io::Error),
}

const POLL_INTERVAL: Duration = Duration::from_millis(5);

/// Runs `program` with `args`, capturing output. stdin is closed so a tool
/// that prompts for input fails fast instead of blocking.
pub fn run_captured(
    program: &Path,
    args: &[&str],
    timeout: Duration,
) -> Result<CapturedOutput, RunError> {
    let mut command = tool_command(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command.spawn().map_err(RunError::Spawn)?;
    // Drain both pipes on their own threads: a tool that fills one pipe while
    // we wait on the other would otherwise deadlock.
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());

    let exit_code = wait_with_timeout(&mut child, timeout)?;
    Ok(CapturedOutput {
        exit_code,
        stdout: stdout.join().unwrap_or_default(),
        stderr: stderr.join().unwrap_or_default(),
    })
}

fn drain(pipe: Option<impl Read + Send + 'static>) -> thread::JoinHandle<String> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut bytes);
        }
        String::from_utf8_lossy(&bytes).into_owned()
    })
}

fn wait_with_timeout(child: &mut Child, timeout: Duration) -> Result<Option<i32>, RunError> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait().map_err(RunError::Wait)? {
            return Ok(status.code());
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(RunError::Timeout(timeout));
        }
        thread::sleep(POLL_INTERVAL);
    }
}

/// A command for a console tool that opens no window. The app is a GUI
/// program, so on Windows each tool it starts would otherwise get a terminal
/// window of its own. Everything that starts a tool outside the engine
/// (whose runner sets the same flag) must use this; clippy rejects
/// `Command::new` anywhere else (`clippy.toml`).
pub fn tool_command(program: impl AsRef<OsStr>) -> Command {
    #[allow(clippy::disallowed_methods)] // the one place that may
    let mut command = Command::new(program);
    hide_console_window(&mut command);
    command
}

/// Stops console tools from flashing a window when called from the GUI.
#[cfg(windows)]
fn hide_console_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_console_window(_: &mut Command) {}

#[cfg(test)]
pub(crate) mod test_support {
    use std::path::{Path, PathBuf};

    /// Writes a stub "tool" script that prints `output` and exits with `code`.
    pub fn write_stub(dir: &Path, name: &str, output: &str, code: i32) -> PathBuf {
        let lines: Vec<&str> = output.lines().collect();
        #[cfg(windows)]
        {
            let path = dir.join(format!("{name}.cmd"));
            let mut script = String::from("@echo off\r\n");
            for line in lines {
                script.push_str(&format!("echo {line}\r\n"));
            }
            script.push_str(&format!("exit /b {code}\r\n"));
            std::fs::write(&path, script).unwrap();
            path
        }
        #[cfg(not(windows))]
        {
            use std::os::unix::fs::PermissionsExt;
            let path = dir.join(name);
            let mut script = String::from("#!/bin/sh\n");
            for line in lines {
                script.push_str(&format!("printf '%s\\n' '{line}'\n"));
            }
            script.push_str(&format!("exit {code}\n"));
            std::fs::write(&path, script).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            path
        }
    }

    /// A stub that runs for several seconds, for timeout tests.
    pub fn write_slow_stub(dir: &Path) -> PathBuf {
        #[cfg(windows)]
        {
            let path = dir.join("slow.cmd");
            std::fs::write(&path, "@echo off\r\nping -n 6 127.0.0.1 > nul\r\n").unwrap();
            path
        }
        #[cfg(not(windows))]
        {
            use std::os::unix::fs::PermissionsExt;
            let path = dir.join("slow");
            std::fs::write(&path, "#!/bin/sh\nsleep 5\n").unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            path
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::*;
    use super::*;

    const GENEROUS: Duration = Duration::from_secs(10);

    #[test]
    fn captures_output_and_exit_code() {
        let dir = tempfile::tempdir().unwrap();
        let stub = write_stub(dir.path(), "tool", "line one\nline two", 3);
        let output = run_captured(&stub, &[], GENEROUS).unwrap();
        assert_eq!(output.exit_code, Some(3));
        assert_eq!(
            output.stdout.lines().collect::<Vec<_>>(),
            ["line one", "line two"]
        );
    }

    #[test]
    fn missing_program_is_a_spawn_error() {
        let dir = tempfile::tempdir().unwrap();
        let result = run_captured(&dir.path().join("does-not-exist"), &[], GENEROUS);
        assert!(matches!(result, Err(RunError::Spawn(_))));
    }

    #[test]
    fn slow_program_is_stopped_at_the_timeout() {
        let dir = tempfile::tempdir().unwrap();
        let stub = write_slow_stub(dir.path());
        let started = Instant::now();
        let result = run_captured(&stub, &[], Duration::from_millis(300));
        assert!(matches!(result, Err(RunError::Timeout(_))), "{result:?}");
        assert!(
            started.elapsed() < Duration::from_secs(4),
            "took {:?}",
            started.elapsed()
        );
    }
}
