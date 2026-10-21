//! `oxcop --server`: one long-lived process answering many command lines,
//! so the spec shim doesn't spawn a process per example. Not a RuboCop
//! feature (RuboCop's `--server` is a different, socket-based daemon).
//!
//! The protocol is length-prefixed so sources need not be valid UTF-8. Each
//! field is `<decimal byte length>\n<bytes>`. A request is
//!
//! ```text
//! <argument count>\n  argument fields...  cwd field  stdin field
//! ```
//!
//! and the reply is `<exit status>\n` followed by the stdout and stderr
//! fields. The server exits when its stdin closes.

use std::ffi::OsString;
use std::io::{BufRead, BufReader, Cursor, Write};
use std::panic::AssertUnwindSafe;

use crate::cli::{Io, Sink};

/// The exit status of a Rust process that panicked.
const STATUS_PANIC: i32 = 101;

pub fn serve() -> i32 {
    // A panic in one request becomes that request's stderr (see
    // `cli::install_panic_hook`) and exit status, as in a process of its own.
    let mut input = BufReader::new(std::io::stdin().lock());
    let mut output = std::io::stdout().lock();
    loop {
        let request = match read_request(&mut input) {
            Ok(Some(request)) => request,
            Ok(None) => return crate::cli::STATUS_SUCCESS,
            Err(e) => {
                eprintln!("oxcop --server: {e}");
                return crate::cli::STATUS_ERROR;
            }
        };
        let (status, stdout, stderr) = handle(request);
        let reply = (|| {
            writeln!(output, "{status}")?;
            write_field(&mut output, &stdout)?;
            write_field(&mut output, &stderr)?;
            output.flush()
        })();
        if reply.is_err() {
            return crate::cli::STATUS_ERROR;
        }
    }
}

struct Request {
    args: Vec<OsString>,
    cwd: OsString,
    stdin: Vec<u8>,
}

fn handle(request: Request) -> (i32, Vec<u8>, Vec<u8>) {
    let stdout = Sink::buffer();
    let stderr = Sink::buffer();
    if let Err(e) = std::env::set_current_dir(&request.cwd) {
        let message = format!(
            "oxcop --server: cd {}: {e}\n",
            request.cwd.to_string_lossy()
        );
        return (crate::cli::STATUS_ERROR, Vec::new(), message.into_bytes());
    }
    let mut io = Io {
        stdin: Box::new(Cursor::new(request.stdin)),
        stdout: stdout.clone(),
        stderr: stderr.clone(),
    };
    let args = std::iter::once(OsString::from("oxcop")).chain(request.args);
    let status = std::panic::catch_unwind(AssertUnwindSafe(|| crate::cli::run_with(args, &mut io)))
        .unwrap_or(STATUS_PANIC);
    (status, stdout.take(), stderr.take())
}

fn read_request(input: &mut impl BufRead) -> std::io::Result<Option<Request>> {
    let Some(argc) = read_length(input)? else {
        return Ok(None);
    };
    let mut args = Vec::with_capacity(argc);
    for _ in 0..argc {
        args.push(os_string(read_field(input)?));
    }
    let cwd = os_string(read_field(input)?);
    let stdin = read_field(input)?;
    Ok(Some(Request { args, cwd, stdin }))
}

/// `None` at end of input.
fn read_length(input: &mut impl BufRead) -> std::io::Result<Option<usize>> {
    let mut line = String::new();
    if input.read_line(&mut line)? == 0 {
        return Ok(None);
    }
    line.trim_end()
        .parse()
        .map(Some)
        .map_err(|_| std::io::Error::other(format!("bad length {line:?}")))
}

fn read_field(input: &mut impl BufRead) -> std::io::Result<Vec<u8>> {
    let length = read_length(input)?.ok_or(std::io::ErrorKind::UnexpectedEof)?;
    let mut bytes = vec![0; length];
    input.read_exact(&mut bytes)?;
    Ok(bytes)
}

fn write_field(output: &mut impl Write, bytes: &[u8]) -> std::io::Result<()> {
    writeln!(output, "{}", bytes.len())?;
    output.write_all(bytes)
}

#[cfg(unix)]
fn os_string(bytes: Vec<u8>) -> OsString {
    use std::os::unix::ffi::OsStringExt;
    OsString::from_vec(bytes)
}

#[cfg(not(unix))]
fn os_string(bytes: Vec<u8>) -> OsString {
    OsString::from(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(args: &[&str], stdin: &str) -> Vec<u8> {
        let mut out = Vec::new();
        writeln!(out, "{}", args.len()).unwrap();
        for arg in args {
            write_field(&mut out, arg.as_bytes()).unwrap();
        }
        let cwd = std::env::current_dir().unwrap();
        write_field(&mut out, cwd.to_str().unwrap().as_bytes()).unwrap();
        write_field(&mut out, stdin.as_bytes()).unwrap();
        out
    }

    #[test]
    fn reads_requests_until_eof() {
        let mut bytes = request(&["--version"], "");
        bytes.extend(request(&["--stdin", "a.rb"], "x = \"\u{e9}\"\n"));
        let mut input = Cursor::new(bytes);
        let first = read_request(&mut input).unwrap().unwrap();
        assert_eq!(first.args, vec![OsString::from("--version")]);
        let second = read_request(&mut input).unwrap().unwrap();
        assert_eq!(second.args.len(), 2);
        assert_eq!(second.stdin, "x = \"\u{e9}\"\n".as_bytes());
        assert!(read_request(&mut input).unwrap().is_none());
    }

    #[test]
    fn handles_a_request() {
        let mut input = Cursor::new(request(&["--version"], ""));
        let (status, stdout, stderr) = handle(read_request(&mut input).unwrap().unwrap());
        assert_eq!(status, 0);
        assert_eq!(
            String::from_utf8(stdout).unwrap(),
            format!("{}\n", crate::version::VERSION)
        );
        assert!(stderr.is_empty());
    }
}
