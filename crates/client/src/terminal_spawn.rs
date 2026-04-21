use std::env;
use std::fs::OpenOptions;
use std::io::{self, IsTerminal, Write};
use std::process::Command;

use crate::paths;

const SENTINEL_ENV: &str = "GUESS_UP_SPAWNED";
const SKIP_FLAG: &str = "--no-spawn-terminal";
const ERROR_LOG_FILENAME: &str = ".guess_up_launch_error.log";

pub enum SpawnOutcome {
    ShouldContinue,
    Spawned,
    Failed,
}

pub fn spawn_if_needed(skip: bool) -> SpawnOutcome {
    if skip || already_spawned() || has_tty() {
        return SpawnOutcome::ShouldContinue;
    }

    let exe = match env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            log_error(&format!("unable to resolve current_exe: {e}"));
            return SpawnOutcome::Failed;
        }
    };

    let forwarded: Vec<String> = env::args().skip(1).filter(|a| a != SKIP_FLAG).collect();

    #[cfg(unix)]
    {
        match try_spawn_linux(&exe, &forwarded) {
            Ok(()) => SpawnOutcome::Spawned,
            Err(reason) => {
                log_error(&format!("linux terminal spawn failed: {reason}"));
                SpawnOutcome::Failed
            }
        }
    }

    #[cfg(windows)]
    {
        match try_spawn_windows(&exe, &forwarded) {
            Ok(()) => SpawnOutcome::Spawned,
            Err(reason) => {
                log_error(&format!("windows terminal spawn failed: {reason}"));
                SpawnOutcome::Failed
            }
        }
    }

    #[cfg(not(any(unix, windows)))]
    {
        let _ = (&exe, &forwarded);
        log_error("terminal spawn not supported on this platform");
        SpawnOutcome::Failed
    }
}

fn has_tty() -> bool {
    io::stdout().is_terminal() || io::stdin().is_terminal()
}

fn already_spawned() -> bool {
    env::var_os(SENTINEL_ENV).is_some()
}

fn log_error(msg: &str) {
    let Ok(dir) = paths::install_dir() else {
        return;
    };
    let path = dir.join(ERROR_LOG_FILENAME);
    let timestamp = chrono::Local::now().to_rfc3339();
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "[{timestamp}] terminal spawn failed: {msg}");
    }
}

#[cfg(unix)]
fn try_spawn_linux(exe: &std::path::Path, forwarded: &[String]) -> Result<(), String> {
    let exe_str = exe.to_string_lossy().into_owned();

    // 1. $TERMINAL
    if let Some(term) = env::var_os("TERMINAL") {
        let term_str = term.to_string_lossy().into_owned();
        if !term_str.is_empty() {
            let mut cmd = Command::new(&term_str);
            cmd.arg("-e").arg(&exe_str).args(forwarded);
            match run_detached(&mut cmd) {
                Ok(()) => return Ok(()),
                Err(SpawnErr::NotFound) => {}
                Err(SpawnErr::Other(e)) => {
                    return Err(format!("$TERMINAL ({term_str}) failed: {e}"));
                }
            }
        }
    }

    // 2. xdg-terminal-exec
    {
        let mut cmd = Command::new("xdg-terminal-exec");
        cmd.arg(&exe_str).args(forwarded);
        match run_detached(&mut cmd) {
            Ok(()) => return Ok(()),
            Err(SpawnErr::NotFound) => {}
            Err(SpawnErr::Other(e)) => {
                return Err(format!("xdg-terminal-exec failed: {e}"));
            }
        }
    }

    // 3. Built-in fallback list
    let attempts: &[(&str, Invocation)] = &[
        ("foot", Invocation::Direct),
        ("alacritty", Invocation::DashE),
        ("kitty", Invocation::Direct),
        ("wezterm", Invocation::WeztermStart),
        ("gnome-terminal", Invocation::DashDash),
        ("konsole", Invocation::DashE),
        ("xfce4-terminal", Invocation::DashE),
        ("tilix", Invocation::DashE),
        ("terminator", Invocation::DashX),
        ("mate-terminal", Invocation::DashE),
        ("lxterminal", Invocation::DashE),
        ("xterm", Invocation::DashE),
    ];

    let mut last_err: Option<String> = None;
    for (bin, style) in attempts {
        let mut cmd = Command::new(bin);
        match style {
            Invocation::Direct => {
                cmd.arg(&exe_str).args(forwarded);
            }
            Invocation::WeztermStart => {
                cmd.arg("start").arg("--").arg(&exe_str).args(forwarded);
            }
            Invocation::DashDash => {
                cmd.arg("--").arg(&exe_str).args(forwarded);
            }
            Invocation::DashE => {
                cmd.arg("-e").arg(&exe_str).args(forwarded);
            }
            Invocation::DashX => {
                cmd.arg("-x").arg(&exe_str).args(forwarded);
            }
        }
        match run_detached(&mut cmd) {
            Ok(()) => return Ok(()),
            Err(SpawnErr::NotFound) => continue,
            Err(SpawnErr::Other(e)) => {
                last_err = Some(format!("{bin} failed: {e}"));
            }
        }
    }

    Err(last_err.unwrap_or_else(|| "no terminal emulator found on PATH".to_string()))
}

#[cfg(unix)]
enum Invocation {
    Direct,
    WeztermStart,
    DashDash,
    DashE,
    DashX,
}

enum SpawnErr {
    NotFound,
    Other(io::Error),
}

fn run_detached(cmd: &mut Command) -> Result<(), SpawnErr> {
    cmd.env(SENTINEL_ENV, "1");
    match cmd.spawn() {
        Ok(_child) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Err(SpawnErr::NotFound),
        Err(e) => Err(SpawnErr::Other(e)),
    }
}

#[cfg(windows)]
fn try_spawn_windows(exe: &std::path::Path, forwarded: &[String]) -> Result<(), String> {
    let exe_str = exe.to_string_lossy().into_owned();

    // 1. Windows Terminal
    {
        let mut cmd = Command::new("wt.exe");
        cmd.arg("new-tab").arg(&exe_str).args(forwarded);
        match run_detached(&mut cmd) {
            Ok(()) => return Ok(()),
            Err(SpawnErr::NotFound) => {}
            Err(SpawnErr::Other(e)) => return Err(format!("wt.exe failed: {e}")),
        }
    }

    // 2. cmd.exe /c start
    {
        let mut cmd = Command::new("cmd.exe");
        cmd.arg("/c")
            .arg("start")
            .arg("")
            .arg(&exe_str)
            .args(forwarded);
        match run_detached(&mut cmd) {
            Ok(()) => return Ok(()),
            Err(SpawnErr::NotFound) => {}
            Err(SpawnErr::Other(e)) => return Err(format!("cmd.exe /c start failed: {e}")),
        }
    }

    Err("neither wt.exe nor cmd.exe /c start succeeded".to_string())
}
