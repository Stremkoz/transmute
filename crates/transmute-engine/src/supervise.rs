//! Running one tool invocation under supervision.
//!
//! The tool runs in its own Windows job object or Unix process group, so
//! cancelling kills it *and anything it started* (ImageMagick, for one,
//! launches helper programs). 1.x only stopped waiting; the encoders kept
//! running and writing.

use std::io;
use std::process::Stdio;

use process_wrap::tokio::*;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio_util::sync::CancellationToken;

use transmute_backends::invoke::Invocation;

/// How much of a tool's output to keep for error messages.
const OUTPUT_TAIL_BYTES: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessEnd {
    Exited {
        /// `None` if the process was killed by a signal.
        code: Option<i32>,
        /// The end of stdout and stderr, for diagnosing failures.
        output: String,
    },
    Cancelled,
}

pub async fn run_supervised(
    invocation: &Invocation,
    cancel: &CancellationToken,
) -> io::Result<ProcessEnd> {
    let mut command = CommandWrap::with_new(&invocation.program, |command| {
        command
            .args(&invocation.args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
    });
    #[cfg(windows)]
    {
        // No console window per tool when the GUI starts one.
        command.wrap(CreationFlags(
            windows::Win32::System::Threading::CREATE_NO_WINDOW,
        ));
        command.wrap(JobObject);
    }
    #[cfg(unix)]
    command.wrap(ProcessGroup::leader());
    command.wrap(KillOnDrop);

    let mut child = command.spawn()?;
    let stdout = tokio::spawn(read_tail(child.stdout().take()));
    let stderr = tokio::spawn(read_tail(child.stderr().take()));

    let status = {
        let wait = child.wait();
        tokio::select! {
            status = wait => Some(status?),
            () = cancel.cancelled() => None,
        }
    };
    let Some(status) = status else {
        Box::into_pin(child.kill()).await?;
        return Ok(ProcessEnd::Cancelled);
    };

    let mut output = stdout.await.unwrap_or_default();
    output.push_str(&stderr.await.unwrap_or_default());
    Ok(ProcessEnd::Exited {
        code: status.code(),
        output: output.trim().to_owned(),
    })
}

/// Reads a pipe to the end, keeping only the last few kilobytes.
async fn read_tail(pipe: Option<impl AsyncRead + Unpin>) -> String {
    let Some(mut pipe) = pipe else {
        return String::new();
    };
    let mut tail = Vec::new();
    let mut chunk = [0u8; 8192];
    while let Ok(n) = pipe.read(&mut chunk).await {
        if n == 0 {
            break;
        }
        tail.extend_from_slice(&chunk[..n]);
        if tail.len() > OUTPUT_TAIL_BYTES * 2 {
            tail.drain(..tail.len() - OUTPUT_TAIL_BYTES);
        }
    }
    let start = tail.len().saturating_sub(OUTPUT_TAIL_BYTES);
    String::from_utf8_lossy(&tail[start..]).into_owned()
}
