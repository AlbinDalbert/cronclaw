use std::path::Path;
use std::process::Command;

/// Resolve the openclaw binary. Checks `OPENCLAW_BIN` env var first,
/// falls back to `openclaw` (found via PATH).
pub fn resolve_binary() -> String {
    std::env::var("OPENCLAW_BIN").unwrap_or_else(|_| "openclaw".to_string())
}

/// Build an `openclaw agent` Command ready to spawn.
///
/// Maps the pipeline's `agent` field to `--to` (agent routing) and passes
/// the resolved prompt via `--message`. Runs in `--local` mode (no gateway).
/// Passes `--timeout` so openclaw can shut down gracefully before cronclaw's
/// hard kill.
///
/// The binary can be overridden via the `OPENCLAW_BIN` environment variable.
pub fn build_command(agent: &str, prompt: &str, workspace: &Path, timeout_secs: u64) -> Command {
    let bin = resolve_binary();
    let mut cmd = Command::new(bin);
    cmd.arg("agent")
        .arg("--message")
        .arg(prompt)
        .arg("--to")
        .arg(agent)
        .arg("--local")
        .arg("--timeout")
        .arg(timeout_secs.to_string())
        .current_dir(workspace);
    cmd
}
