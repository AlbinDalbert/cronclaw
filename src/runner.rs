use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use regex::Regex;

use crate::config::Config;
use crate::pipeline::{Step, StepType};
use crate::state::{self, State, StepStatus};

pub fn run_pipeline(pipeline_dir: &Path, cfg: &Config, verbose: bool) -> Result<(), String> {
    let pipeline_file = pipeline_dir.join("pipeline.yaml");
    let state_file = pipeline_dir.join("state.json");
    let pipeline_name = pipeline_dir
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();

    let pipeline = crate::pipeline::load(&pipeline_file)?;
    let workspace = pipeline_dir.join(&pipeline.workspace);

    // Load or create state
    let mut state = match state::load(&state_file)? {
        Some(s) => s,
        None => {
            fs::create_dir_all(&workspace)
                .map_err(|e| format!("failed to create workspace: {}", e))?;
            let s = State::from_pipeline(&pipeline);
            state::save(&state_file, &s)?;
            s
        }
    };

    // Verify state matches pipeline
    {
        let pipeline_ids: std::collections::BTreeSet<&str> =
            pipeline.steps.iter().map(|s| s.id.as_str()).collect();
        let state_ids: std::collections::BTreeSet<&str> =
            state.steps.keys().map(|s| s.as_str()).collect();

        if pipeline_ids != state_ids {
            return Err(format!(
                "[{}] state file mismatch — steps in pipeline.yaml don't match state.json. \
                 Consider resetting the pipeline with `cronclaw reset {}`.",
                pipeline_name, pipeline_name
            ));
        }
    }

    // Walk steps in pipeline order, find the first non-completed one
    for (i, step) in pipeline.steps.iter().enumerate() {
        let step_state = &state.steps[&step.id];

        match step_state.status {
            StepStatus::Completed => continue,
            StepStatus::Running => {
                if verbose {
                    println!(
                        "[{}] step '{}' is already running — exiting",
                        pipeline_name, step.id
                    );
                }
                return Ok(());
            }
            StepStatus::Failed => {
                if verbose {
                    println!(
                        "[{}] step '{}' is in failed state — skipping pipeline",
                        pipeline_name, step.id
                    );
                }
                return Ok(());
            }
            StepStatus::Pending => {
                let timeout_secs = step.timeout.unwrap_or(cfg.timeout);

                println!(
                    "[{}] running step {}/{}: '{}' ({})",
                    pipeline_name,
                    i + 1,
                    pipeline.steps.len(),
                    step.id,
                    match step.step_type {
                        StepType::Bash => "bash",
                        StepType::Agent => "agent",
                    }
                );

                // Mark as running before execution
                state
                    .steps
                    .get_mut(&step.id)
                    .unwrap()
                    .status = StepStatus::Running;
                state::save(&state_file, &state)?;

                // Execute
                match execute_step(step, &workspace, timeout_secs) {
                    Ok(()) => {
                        promote_outputs(step, &workspace)?;

                        state
                            .steps
                            .get_mut(&step.id)
                            .unwrap()
                            .status = StepStatus::Completed;
                        state::save(&state_file, &state)?;

                        // Check if that was the last step
                        let all_done = pipeline.steps.iter().all(|s| {
                            state
                                .steps
                                .get(&s.id)
                                .map(|ss| ss.status == StepStatus::Completed)
                                .unwrap_or(false)
                        });
                        if all_done {
                            println!("[{}] pipeline completed", pipeline_name);
                        }
                    }
                    Err(e) => {
                        state
                            .steps
                            .get_mut(&step.id)
                            .unwrap()
                            .status = StepStatus::Failed;
                        state::save(&state_file, &state)?;

                        return Err(format!(
                            "[{}] step '{}' failed: {}",
                            pipeline_name, step.id, e
                        ));
                    }
                }

                return Ok(());
            }
        }
    }

    // All steps completed already — silent unless verbose
    if verbose {
        println!("[{}] pipeline already completed", pipeline_name);
    }
    Ok(())
}

fn execute_step(step: &Step, workspace: &Path, timeout_secs: u64) -> Result<(), String> {
    match step.step_type {
        StepType::Bash => {
            let script = step.bash.as_ref().unwrap();
            run_with_timeout(
                Command::new("sh")
                    .arg("-c")
                    .arg(script)
                    .current_dir(workspace),
                timeout_secs,
            )
        }
        StepType::Agent => {
            let agent = step.agent.as_ref().unwrap();
            let raw_prompt = step.prompt.as_ref().unwrap();
            let prompt = resolve_templates(raw_prompt, workspace)?;

            // TODO: integrate with OpenClaw agent runtime
            eprintln!("agent execution not yet implemented");
            eprintln!("  agent: {}", agent);
            eprintln!("  prompt: {}", prompt.lines().next().unwrap_or("(empty)"));
            Err("agent steps are not yet implemented".to_string())
        }
    }
}

fn run_with_timeout(cmd: &mut Command, timeout_secs: u64) -> Result<(), String> {
    let mut child = cmd
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to spawn: {}", e))?;

    let timeout = Duration::from_secs(timeout_secs);
    let start = Instant::now();

    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                // Process finished — collect output
                let output = child
                    .wait_with_output()
                    .map_err(|e| format!("failed to read output: {}", e))?;

                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);

                if !stdout.is_empty() {
                    print!("{}", stdout);
                }
                if !stderr.is_empty() {
                    eprint!("{}", stderr);
                }

                if status.success() {
                    return Ok(());
                } else {
                    return Err(format!(
                        "exited with code {}",
                        status.code().unwrap_or(-1)
                    ));
                }
            }
            Ok(None) => {
                // Still running
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!("timed out after {}s", timeout_secs));
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => {
                return Err(format!("failed to check process status: {}", e));
            }
        }
    }
}

/// Replace {{ file:path }} with the contents of the file relative to workspace.
pub fn resolve_templates(input: &str, workspace: &Path) -> Result<String, String> {
    let re = Regex::new(r"\{\{\s*file:\s*(.+?)\s*\}\}").unwrap();
    let mut result = input.to_string();

    // Collect matches first to avoid borrow issues
    let matches: Vec<(String, String)> = re
        .captures_iter(input)
        .map(|cap| {
            let full_match = cap[0].to_string();
            let file_path = cap[1].to_string();
            (full_match, file_path)
        })
        .collect();

    for (full_match, file_path) in matches {
        let path = workspace.join(&file_path);
        let content = fs::read_to_string(&path).map_err(|e| {
            format!(
                "template '{}': failed to read '{}': {}",
                full_match,
                path.display(),
                e
            )
        })?;
        result = result.replace(&full_match, &content);
    }

    Ok(result)
}

pub fn promote_outputs(step: &Step, workspace: &Path) -> Result<(), String> {
    for output in &step.outputs {
        let tmp_path = workspace.join(&output.tmp);
        let final_path = workspace.join(&output.path);

        if !tmp_path.exists() {
            return Err(format!(
                "output '{}': tmp file '{}' not found after step completed",
                output.name, output.tmp
            ));
        }

        fs::rename(&tmp_path, &final_path).map_err(|e| {
            format!(
                "output '{}': failed to promote '{}' -> '{}': {}",
                output.name, output.tmp, output.path, e
            )
        })?;
    }
    Ok(())
}
