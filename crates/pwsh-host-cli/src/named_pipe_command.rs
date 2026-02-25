use std::error::Error;
use std::ffi::{OsStr, OsString};
use std::fmt::{Display, Formatter};
use std::time::Duration;

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};

const NAMED_PIPE_COMMAND_FLAG: &str = "-namedpipecommand";
const ENCODED_COMMAND_FLAG: &str = "-EncodedCommand";
const PIPE_CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_COMMAND_BYTES: usize = 1024 * 1024;

#[derive(Debug)]
pub struct NamedPipeCommandError {
    message: String,
}

impl NamedPipeCommandError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl Display for NamedPipeCommandError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl Error for NamedPipeCommandError {}

#[derive(Debug)]
struct NamedPipeCommandArg {
    index: usize,
    pipe_name: OsString,
}

pub fn preprocess_named_pipe_command_args(args: Vec<OsString>) -> Result<Vec<OsString>, NamedPipeCommandError> {
    let named_pipe_arg = match parse_named_pipe_command_arg(&args)? {
        Some(arg) => arg,
        None => return Ok(args),
    };

    let command = read_named_pipe_command(named_pipe_arg.pipe_name.as_os_str())?;
    let encoded = encode_utf16le_base64(&command);

    Ok(rewrite_with_encoded_command(&args, named_pipe_arg.index, &encoded))
}

fn parse_named_pipe_command_arg(args: &[OsString]) -> Result<Option<NamedPipeCommandArg>, NamedPipeCommandError> {
    let mut found: Option<NamedPipeCommandArg> = None;

    for (index, arg) in args.iter().enumerate() {
        if !is_named_pipe_command_flag(arg.as_os_str()) {
            continue;
        }

        if found.is_some() {
            return Err(NamedPipeCommandError::new(
                "-NamedPipeCommand can only be specified once",
            ));
        }

        let value = args
            .get(index + 1)
            .ok_or_else(|| NamedPipeCommandError::new("-NamedPipeCommand requires a named pipe value"))?;

        if is_option_like(value.as_os_str()) {
            return Err(NamedPipeCommandError::new(
                "-NamedPipeCommand requires a named pipe value",
            ));
        }

        found = Some(NamedPipeCommandArg {
            index,
            pipe_name: value.clone(),
        });
    }

    if let Some(named_pipe_arg) = found {
        ensure_no_command_source_conflicts(args, named_pipe_arg.index)?;
        Ok(Some(named_pipe_arg))
    } else {
        Ok(None)
    }
}

fn ensure_no_command_source_conflicts(args: &[OsString], named_pipe_index: usize) -> Result<(), NamedPipeCommandError> {
    for (index, arg) in args.iter().enumerate() {
        if index == named_pipe_index || index == named_pipe_index + 1 {
            continue;
        }

        let normalized = normalize_flag(arg.as_os_str());
        if is_command_source_flag(&normalized) {
            return Err(NamedPipeCommandError::new(format!(
                "-NamedPipeCommand cannot be combined with {}",
                arg.to_string_lossy()
            )));
        }
    }

    Ok(())
}

fn rewrite_with_encoded_command(args: &[OsString], named_pipe_index: usize, encoded_command: &str) -> Vec<OsString> {
    let mut rewritten = Vec::with_capacity(args.len());

    rewritten.extend_from_slice(&args[..named_pipe_index]);
    rewritten.push(OsString::from(ENCODED_COMMAND_FLAG));
    rewritten.push(OsString::from(encoded_command));
    rewritten.extend_from_slice(&args[named_pipe_index + 2..]);

    rewritten
}

fn encode_utf16le_base64(command: &str) -> String {
    let bytes: Vec<u8> = command.encode_utf16().flat_map(|value| value.to_le_bytes()).collect();
    BASE64_STANDARD.encode(bytes)
}

#[cfg(windows)]
fn read_named_pipe_command(pipe_name: &OsStr) -> Result<String, NamedPipeCommandError> {
    use std::fs::OpenOptions;
    use std::io::Read;
    use std::thread::sleep;
    use std::time::Instant;

    let pipe_name = pipe_name.to_string_lossy();
    if pipe_name.trim().is_empty() {
        return Err(NamedPipeCommandError::new(
            "-NamedPipeCommand requires a non-empty named pipe value",
        ));
    }

    let pipe_path = format!(r"\\.\pipe\{}", pipe_name);
    let deadline = Instant::now() + PIPE_CONNECT_TIMEOUT;

    loop {
        match OpenOptions::new().read(true).open(&pipe_path) {
            Ok(mut pipe) => {
                let mut bytes = Vec::new();
                pipe.read_to_end(&mut bytes).map_err(|error| {
                    NamedPipeCommandError::new(format!(
                        "failed reading command from named pipe '{}': {}",
                        pipe_path, error
                    ))
                })?;

                if bytes.is_empty() {
                    return Err(NamedPipeCommandError::new(format!(
                        "named pipe '{}' returned an empty command",
                        pipe_path
                    )));
                }

                if bytes.len() > MAX_COMMAND_BYTES {
                    return Err(NamedPipeCommandError::new(format!(
                        "named pipe command exceeded {} bytes",
                        MAX_COMMAND_BYTES
                    )));
                }

                let command = String::from_utf8(bytes).map_err(|_| {
                    NamedPipeCommandError::new(format!("named pipe '{}' did not return valid UTF-8", pipe_path))
                })?;

                return Ok(command);
            }
            Err(error) => {
                let code = error.raw_os_error().unwrap_or_default();
                let retryable = matches!(code, 2 | 53 | 231);

                if retryable && Instant::now() < deadline {
                    sleep(Duration::from_millis(25));
                    continue;
                }

                if retryable {
                    return Err(NamedPipeCommandError::new(format!(
                        "timed out waiting for named pipe '{}'",
                        pipe_path
                    )));
                }

                return Err(NamedPipeCommandError::new(format!(
                    "failed opening named pipe '{}': {}",
                    pipe_path, error
                )));
            }
        }
    }
}

#[cfg(not(windows))]
fn read_named_pipe_command(_pipe_name: &OsStr) -> Result<String, NamedPipeCommandError> {
    Err(NamedPipeCommandError::new(
        "-NamedPipeCommand is currently supported on Windows only",
    ))
}

fn is_named_pipe_command_flag(arg: &OsStr) -> bool {
    normalize_flag(arg) == NAMED_PIPE_COMMAND_FLAG
}

fn is_command_source_flag(arg: &str) -> bool {
    matches!(
        arg,
        "-encodedcommand" | "-e" | "-ec" | "-command" | "-c" | "-file" | "-f" | "-commandwithargs" | "-cwa"
    )
}

fn is_option_like(arg: &OsStr) -> bool {
    let text = arg.to_string_lossy();
    text.starts_with('-') || text.starts_with('/')
}

fn normalize_flag(arg: &OsStr) -> String {
    arg.to_string_lossy().to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::{encode_utf16le_base64, parse_named_pipe_command_arg};

    fn os_args(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    #[test]
    fn no_named_pipe_flag_keeps_args() {
        let args = os_args(&["-NoProfile", "-Version"]);
        let parsed = parse_named_pipe_command_arg(&args).unwrap();
        assert!(parsed.is_none());
    }

    #[test]
    fn duplicate_named_pipe_flag_is_rejected() {
        let args = os_args(&["-NamedPipeCommand", "pipe1", "-NamedPipeCommand", "pipe2"]);
        let error = parse_named_pipe_command_arg(&args).unwrap_err();
        assert!(error.to_string().contains("only be specified once"));
    }

    #[test]
    fn missing_named_pipe_value_is_rejected() {
        let args = os_args(&["-NamedPipeCommand"]);
        let error = parse_named_pipe_command_arg(&args).unwrap_err();
        assert!(error.to_string().contains("requires a named pipe value"));
    }

    #[test]
    fn command_source_conflict_is_rejected() {
        let args = os_args(&["-NamedPipeCommand", "pipe", "-Command", "'test'"]);
        let error = parse_named_pipe_command_arg(&args).unwrap_err();
        assert!(error.to_string().contains("cannot be combined"));
    }

    #[test]
    fn utf16_base64_encoding_matches_expected() {
        assert_eq!(encode_utf16le_base64("ab"), "YQBiAA==");
    }

    #[cfg(not(windows))]
    #[test]
    fn named_pipe_flag_is_windows_only() {
        let args = os_args(&["-NamedPipeCommand", "pipe"]);
        let error = super::preprocess_named_pipe_command_args(args).unwrap_err();
        assert!(error.to_string().contains("Windows only"));
    }
}
