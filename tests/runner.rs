use cronclaw::config::Config;
use cronclaw::pipeline;
use cronclaw::runner;
use cronclaw::state::{self, State, StepStatus};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::sync::Mutex;
use tempfile::TempDir;

/// Mutex to serialize agent tests that mutate OPENCLAW_BIN env var.
static OPENCLAW_BIN_LOCK: Mutex<()> = Mutex::new(());

// ─── Template resolution ───

#[test]
fn resolve_single_template() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("notes.md"), "hello world").unwrap();

    let input = "Read this: {{ file:notes.md }}";
    let result = runner::resolve_templates(input, dir.path()).unwrap();
    assert_eq!(result, "Read this: hello world");
}

#[test]
fn resolve_multiple_templates() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a.txt"), "AAA").unwrap();
    fs::write(dir.path().join("b.txt"), "BBB").unwrap();

    let input = "First: {{ file:a.txt }} Second: {{ file:b.txt }}";
    let result = runner::resolve_templates(input, dir.path()).unwrap();
    assert_eq!(result, "First: AAA Second: BBB");
}

#[test]
fn resolve_template_with_spaces() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("data.txt"), "content").unwrap();

    // Various whitespace inside the braces
    let result = runner::resolve_templates("{{file:data.txt}}", dir.path()).unwrap();
    assert_eq!(result, "content");

    let result = runner::resolve_templates("{{  file:  data.txt  }}", dir.path()).unwrap();
    assert_eq!(result, "content");

    let result = runner::resolve_templates("{{ file: data.txt }}", dir.path()).unwrap();
    assert_eq!(result, "content");
}

#[test]
fn resolve_template_missing_file_errors() {
    let dir = TempDir::new().unwrap();
    let result = runner::resolve_templates("{{ file:missing.txt }}", dir.path());
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("missing.txt"));
}

#[test]
fn resolve_no_templates_passthrough() {
    let dir = TempDir::new().unwrap();
    let input = "No templates here, just regular text.";
    let result = runner::resolve_templates(input, dir.path()).unwrap();
    assert_eq!(result, input);
}

#[test]
fn resolve_template_multiline_content() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("multi.txt"), "line 1\nline 2\nline 3").unwrap();

    let result = runner::resolve_templates("Content:\n{{ file:multi.txt }}", dir.path()).unwrap();
    assert!(result.contains("line 1\nline 2\nline 3"));
}

// ─── Output promotion ───

#[test]
fn promote_outputs_renames_tmp_to_final() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("out.txt.tmp"), "data").unwrap();

    let yaml = r#"
version: 1
workspace: workspace
steps:
  - id: s
    type: bash
    bash: echo
    outputs:
      - name: out
        path: out.txt
        tmp: out.txt.tmp
"#;
    let p = pipeline::parse(yaml).unwrap();
    runner::promote_outputs(&p.steps[0], dir.path()).unwrap();

    assert!(!dir.path().join("out.txt.tmp").exists());
    assert_eq!(
        fs::read_to_string(dir.path().join("out.txt")).unwrap(),
        "data"
    );
}

#[test]
fn promote_outputs_missing_tmp_errors() {
    let dir = TempDir::new().unwrap();

    let yaml = r#"
version: 1
workspace: workspace
steps:
  - id: s
    type: bash
    bash: echo
    outputs:
      - name: result
        path: result.txt
        tmp: result.txt.tmp
"#;
    let p = pipeline::parse(yaml).unwrap();
    let err = runner::promote_outputs(&p.steps[0], dir.path()).unwrap_err();
    assert!(err.contains("result"));
    assert!(err.contains("not found"));
}

#[test]
fn promote_no_outputs_succeeds() {
    let dir = TempDir::new().unwrap();

    let yaml = r#"
version: 1
workspace: workspace
steps:
  - id: s
    type: bash
    bash: echo hi
"#;
    let p = pipeline::parse(yaml).unwrap();
    runner::promote_outputs(&p.steps[0], dir.path()).unwrap();
}

// ─── Full pipeline tick behavior ───

fn setup_pipeline(dir: &std::path::Path, yaml: &str) {
    let pipeline_dir = dir.join("pipelines").join("test");
    fs::create_dir_all(&pipeline_dir).unwrap();
    fs::write(pipeline_dir.join("pipeline.yaml"), yaml).unwrap();
}

fn pipeline_dir(dir: &std::path::Path) -> std::path::PathBuf {
    dir.join("pipelines").join("test")
}

#[test]
fn run_single_bash_step_completes() {
    let dir = TempDir::new().unwrap();
    setup_pipeline(
        dir.path(),
        r#"
version: 1
workspace: workspace
steps:
  - id: hello
    type: bash
    bash: echo "hi"
"#,
    );

    let cfg = Config::default();
    runner::run_pipeline(&pipeline_dir(dir.path()), &cfg, false).unwrap();

    let state = state::load(&pipeline_dir(dir.path()).join("state.json"))
        .unwrap()
        .unwrap();
    assert_eq!(state.steps["hello"].status, StepStatus::Completed);
}

#[test]
fn run_advances_one_step_per_tick() {
    let dir = TempDir::new().unwrap();
    setup_pipeline(
        dir.path(),
        r#"
version: 1
workspace: workspace
steps:
  - id: first
    type: bash
    bash: echo 1
  - id: second
    type: bash
    bash: echo 2
  - id: third
    type: bash
    bash: echo 3
"#,
    );

    let cfg = Config::default();
    let pd = pipeline_dir(dir.path());

    // Tick 1
    runner::run_pipeline(&pd, &cfg, false).unwrap();
    let s = state::load(&pd.join("state.json")).unwrap().unwrap();
    assert_eq!(s.steps["first"].status, StepStatus::Completed);
    assert_eq!(s.steps["second"].status, StepStatus::Pending);
    assert_eq!(s.steps["third"].status, StepStatus::Pending);

    // Tick 2
    runner::run_pipeline(&pd, &cfg, false).unwrap();
    let s = state::load(&pd.join("state.json")).unwrap().unwrap();
    assert_eq!(s.steps["first"].status, StepStatus::Completed);
    assert_eq!(s.steps["second"].status, StepStatus::Completed);
    assert_eq!(s.steps["third"].status, StepStatus::Pending);

    // Tick 3
    runner::run_pipeline(&pd, &cfg, false).unwrap();
    let s = state::load(&pd.join("state.json")).unwrap().unwrap();
    assert_eq!(s.steps["first"].status, StepStatus::Completed);
    assert_eq!(s.steps["second"].status, StepStatus::Completed);
    assert_eq!(s.steps["third"].status, StepStatus::Completed);
}

#[test]
fn run_failed_step_blocks_pipeline() {
    let dir = TempDir::new().unwrap();
    setup_pipeline(
        dir.path(),
        r#"
version: 1
workspace: workspace
steps:
  - id: fail
    type: bash
    bash: exit 1
  - id: after
    type: bash
    bash: echo "should not run"
"#,
    );

    let cfg = Config::default();
    let pd = pipeline_dir(dir.path());

    // Tick 1 — step fails
    let result = runner::run_pipeline(&pd, &cfg, false);
    assert!(result.is_err());

    let s = state::load(&pd.join("state.json")).unwrap().unwrap();
    assert_eq!(s.steps["fail"].status, StepStatus::Failed);
    assert_eq!(s.steps["after"].status, StepStatus::Pending);

    // Tick 2 — pipeline is blocked, no progress
    runner::run_pipeline(&pd, &cfg, false).unwrap();
    let s = state::load(&pd.join("state.json")).unwrap().unwrap();
    assert_eq!(s.steps["fail"].status, StepStatus::Failed);
    assert_eq!(s.steps["after"].status, StepStatus::Pending);
}

#[test]
fn run_failed_step_does_not_promote_outputs() {
    let dir = TempDir::new().unwrap();
    setup_pipeline(
        dir.path(),
        r#"
version: 1
workspace: workspace
steps:
  - id: fail
    type: bash
    bash: echo "data" > out.txt.tmp && exit 1
    outputs:
      - name: out
        path: out.txt
        tmp: out.txt.tmp
"#,
    );

    let cfg = Config::default();
    let pd = pipeline_dir(dir.path());
    let workspace = pd.join("workspace");

    let _ = runner::run_pipeline(&pd, &cfg, false);

    // tmp should still exist (not promoted)
    assert!(workspace.join("out.txt.tmp").exists());
    // final should NOT exist
    assert!(!workspace.join("out.txt").exists());
}

#[test]
fn run_state_mismatch_errors() {
    let dir = TempDir::new().unwrap();
    setup_pipeline(
        dir.path(),
        r#"
version: 1
workspace: workspace
steps:
  - id: step-a
    type: bash
    bash: echo a
"#,
    );

    let cfg = Config::default();
    let pd = pipeline_dir(dir.path());

    // Run once to create state
    runner::run_pipeline(&pd, &cfg, false).unwrap();

    // Change pipeline to have different steps
    fs::write(
        pd.join("pipeline.yaml"),
        r#"
version: 1
workspace: workspace
steps:
  - id: step-b
    type: bash
    bash: echo b
"#,
    )
    .unwrap();

    let result = runner::run_pipeline(&pd, &cfg, false);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.contains("mismatch"));
    assert!(err.contains("reset"));
}

#[test]
fn run_running_step_causes_early_exit() {
    let dir = TempDir::new().unwrap();
    setup_pipeline(
        dir.path(),
        r#"
version: 1
workspace: workspace
steps:
  - id: stuck
    type: bash
    bash: echo hi
  - id: next
    type: bash
    bash: echo next
"#,
    );

    let cfg = Config::default();
    let pd = pipeline_dir(dir.path());

    // Create state with 'stuck' as running (simulating a crashed previous run)
    let p = pipeline::parse(&fs::read_to_string(pd.join("pipeline.yaml")).unwrap()).unwrap();
    let mut s = State::from_pipeline(&p);
    s.steps.get_mut("stuck").unwrap().status = StepStatus::Running;
    fs::create_dir_all(pd.join("workspace")).unwrap();
    state::save(&pd.join("state.json"), &s).unwrap();

    // Tick should see 'running' and exit without error, without touching 'next'
    runner::run_pipeline(&pd, &cfg, false).unwrap();

    let s = state::load(&pd.join("state.json")).unwrap().unwrap();
    assert_eq!(s.steps["stuck"].status, StepStatus::Running);
    assert_eq!(s.steps["next"].status, StepStatus::Pending);
}

// ─── Agent step integration ───

/// Create a fake `openclaw` script in a temp dir and return its absolute path.
fn install_fake_openclaw(dir: &std::path::Path, script_body: &str) -> std::path::PathBuf {
    let script_path = dir.join("fake-openclaw");
    fs::write(&script_path, format!("#!/bin/sh\n{}", script_body)).unwrap();
    fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755)).unwrap();
    script_path
}

/// Run a pipeline with OPENCLAW_BIN pointed at a fake script.
/// Uses a mutex so concurrent tests don't clobber each other's env var.
fn run_with_fake_openclaw(
    pipeline_dir: &std::path::Path,
    fake_bin: &std::path::Path,
    cfg: &Config,
) -> Result<(), String> {
    let _guard = OPENCLAW_BIN_LOCK.lock().unwrap();

    // SAFETY: serialized by mutex — no concurrent env mutation.
    unsafe { std::env::set_var("OPENCLAW_BIN", fake_bin) };
    let result = runner::run_pipeline(pipeline_dir, cfg, false);
    unsafe { std::env::remove_var("OPENCLAW_BIN") };

    result
}

#[test]
fn run_agent_step_completes_on_success() {
    let dir = TempDir::new().unwrap();

    let fake_bin = install_fake_openclaw(dir.path(), "exit 0");

    let pd = pipeline_dir(dir.path());
    setup_pipeline(
        dir.path(),
        r#"
version: 1
workspace: workspace
steps:
  - id: analyse
    type: agent
    agent: pro-worker
    prompt: "Analyse this data"
    output: analysis.md
"#,
    );

    let cfg = Config::default();
    run_with_fake_openclaw(&pd, &fake_bin, &cfg).unwrap();

    let s = state::load(&pd.join("state.json")).unwrap().unwrap();
    assert_eq!(s.steps["analyse"].status, StepStatus::Completed);
}

#[test]
fn run_agent_step_fails_on_nonzero_exit() {
    let dir = TempDir::new().unwrap();

    let fake_bin = install_fake_openclaw(dir.path(), "echo 'agent error' >&2\nexit 1");

    let pd = pipeline_dir(dir.path());
    setup_pipeline(
        dir.path(),
        r#"
version: 1
workspace: workspace
steps:
  - id: analyse
    type: agent
    agent: pro-worker
    prompt: "Analyse this data"
    output: analysis.md
"#,
    );

    let cfg = Config::default();
    let result = run_with_fake_openclaw(&pd, &fake_bin, &cfg);
    assert!(result.is_err());

    let s = state::load(&pd.join("state.json")).unwrap().unwrap();
    assert_eq!(s.steps["analyse"].status, StepStatus::Failed);
}

#[test]
fn run_agent_step_resolves_templates() {
    let dir = TempDir::new().unwrap();

    let fake_bin = install_fake_openclaw(
        dir.path(),
        r#"
# Find --message arg value
while [ "$#" -gt 0 ]; do
    case "$1" in
        --message) shift; echo "$1" > "$PWD/received_prompt.txt"; break;;
        *) shift;;
    esac
done
exit 0
"#,
    );

    let pd = pipeline_dir(dir.path());
    setup_pipeline(
        dir.path(),
        r#"
version: 1
workspace: workspace
steps:
  - id: analyse
    type: agent
    agent: worker
    prompt: |
      Here is the data:
      {{ file:data.json }}
    output: analysis.md
"#,
    );

    // Create the workspace and the file to inject
    let workspace = pd.join("workspace");
    fs::create_dir_all(&workspace).unwrap();
    fs::write(workspace.join("data.json"), r#"{"value": 42}"#).unwrap();

    let cfg = Config::default();
    run_with_fake_openclaw(&pd, &fake_bin, &cfg).unwrap();

    // Verify the template was resolved before passing to openclaw
    let received = fs::read_to_string(workspace.join("received_prompt.txt")).unwrap();
    assert!(received.contains(r#"{"value": 42}"#));
    assert!(!received.contains("{{ file:"));
}

#[test]
fn run_agent_step_promotes_outputs() {
    let dir = TempDir::new().unwrap();

    let fake_bin = install_fake_openclaw(
        dir.path(),
        r#"echo "result data" > "$PWD/result.txt.tmp"
exit 0"#,
    );

    let pd = pipeline_dir(dir.path());
    setup_pipeline(
        dir.path(),
        r#"
version: 1
workspace: workspace
steps:
  - id: analyse
    type: agent
    agent: worker
    prompt: "do work"
    output: agent-out.md
    outputs:
      - name: result
        path: result.txt
        tmp: result.txt.tmp
"#,
    );

    let workspace = pd.join("workspace");
    fs::create_dir_all(&workspace).unwrap();

    let cfg = Config::default();
    run_with_fake_openclaw(&pd, &fake_bin, &cfg).unwrap();

    // tmp should be promoted to final
    assert!(!workspace.join("result.txt.tmp").exists());
    assert!(workspace.join("result.txt").exists());
    let content = fs::read_to_string(workspace.join("result.txt")).unwrap();
    assert!(content.contains("result data"));
}

#[test]
fn run_mixed_bash_and_agent_steps() {
    let dir = TempDir::new().unwrap();

    let fake_bin = install_fake_openclaw(dir.path(), "exit 0");

    let pd = pipeline_dir(dir.path());
    setup_pipeline(
        dir.path(),
        r#"
version: 1
workspace: workspace
steps:
  - id: prep
    type: bash
    bash: echo "prepared"
  - id: analyse
    type: agent
    agent: worker
    prompt: "do analysis"
    output: analysis.md
  - id: cleanup
    type: bash
    bash: echo "done"
"#,
    );

    let cfg = Config::default();

    // Tick 1 — bash step
    run_with_fake_openclaw(&pd, &fake_bin, &cfg).unwrap();
    let s = state::load(&pd.join("state.json")).unwrap().unwrap();
    assert_eq!(s.steps["prep"].status, StepStatus::Completed);
    assert_eq!(s.steps["analyse"].status, StepStatus::Pending);

    // Tick 2 — agent step
    run_with_fake_openclaw(&pd, &fake_bin, &cfg).unwrap();
    let s = state::load(&pd.join("state.json")).unwrap().unwrap();
    assert_eq!(s.steps["analyse"].status, StepStatus::Completed);
    assert_eq!(s.steps["cleanup"].status, StepStatus::Pending);

    // Tick 3 — bash step
    run_with_fake_openclaw(&pd, &fake_bin, &cfg).unwrap();
    let s = state::load(&pd.join("state.json")).unwrap().unwrap();
    assert_eq!(s.steps["cleanup"].status, StepStatus::Completed);
}

#[test]
fn run_agent_stdout_captured_to_output_file() {
    let dir = TempDir::new().unwrap();

    let fake_bin = install_fake_openclaw(dir.path(), r#"echo "agent response content""#);

    let pd = pipeline_dir(dir.path());
    setup_pipeline(
        dir.path(),
        r#"
version: 1
workspace: workspace
steps:
  - id: analyse
    type: agent
    agent: worker
    prompt: "do work"
    output: result.md
"#,
    );

    let cfg = Config::default();
    run_with_fake_openclaw(&pd, &fake_bin, &cfg).unwrap();

    let workspace = pd.join("workspace");
    let content = fs::read_to_string(workspace.join("result.md")).unwrap();
    assert!(content.contains("agent response content"));
}

#[test]
fn run_agent_stderr_captured_to_error_file() {
    let dir = TempDir::new().unwrap();

    let fake_bin = install_fake_openclaw(dir.path(), "echo 'some warning' >&2\necho 'response'");

    let pd = pipeline_dir(dir.path());
    setup_pipeline(
        dir.path(),
        r#"
version: 1
workspace: workspace
steps:
  - id: analyse
    type: agent
    agent: worker
    prompt: "do work"
    output: result.md
    error: analyse.err
"#,
    );

    let cfg = Config::default();
    run_with_fake_openclaw(&pd, &fake_bin, &cfg).unwrap();

    let workspace = pd.join("workspace");
    let err_content = fs::read_to_string(workspace.join("analyse.err")).unwrap();
    assert!(err_content.contains("some warning"));
}

#[test]
fn run_agent_stderr_captured_to_custom_error_file() {
    let dir = TempDir::new().unwrap();

    let fake_bin = install_fake_openclaw(dir.path(), "echo 'debug info' >&2\necho 'response'");

    let pd = pipeline_dir(dir.path());
    setup_pipeline(
        dir.path(),
        r#"
version: 1
workspace: workspace
steps:
  - id: analyse
    type: agent
    agent: worker
    prompt: "do work"
    output: result.md
    error: custom-errors.log
"#,
    );

    let cfg = Config::default();
    run_with_fake_openclaw(&pd, &fake_bin, &cfg).unwrap();

    let workspace = pd.join("workspace");
    let err_content = fs::read_to_string(workspace.join("custom-errors.log")).unwrap();
    assert!(err_content.contains("debug info"));
    // Default error file should NOT exist
    assert!(!workspace.join("analyse.err").exists());
}

#[test]
fn run_agent_output_consumable_by_next_step_template() {
    let dir = TempDir::new().unwrap();

    // First agent writes its response to stdout
    let fake_bin = install_fake_openclaw(dir.path(), r#"echo "analysis result 42""#);

    let pd = pipeline_dir(dir.path());
    setup_pipeline(
        dir.path(),
        r#"
version: 1
workspace: workspace
steps:
  - id: analyse
    type: agent
    agent: worker
    prompt: "analyse data"
    output: analysis.md
  - id: report
    type: bash
    bash: cat analysis.md > report.txt
"#,
    );

    let cfg = Config::default();

    // Tick 1 — agent step writes output
    run_with_fake_openclaw(&pd, &fake_bin, &cfg).unwrap();

    // Tick 2 — bash step consumes the agent's output file
    run_with_fake_openclaw(&pd, &fake_bin, &cfg).unwrap();

    let workspace = pd.join("workspace");
    let report = fs::read_to_string(workspace.join("report.txt")).unwrap();
    assert!(report.contains("analysis result 42"));
}

#[test]
fn run_bash_stdout_captured_to_output_file() {
    let dir = TempDir::new().unwrap();

    let pd = pipeline_dir(dir.path());
    setup_pipeline(
        dir.path(),
        r#"
version: 1
workspace: workspace
steps:
  - id: greet
    type: bash
    bash: echo "hello from bash"
    output: greeting.txt
"#,
    );

    let cfg = Config::default();
    runner::run_pipeline(&pd, &cfg, false).unwrap();

    let workspace = pd.join("workspace");
    let content = fs::read_to_string(workspace.join("greeting.txt")).unwrap();
    assert!(content.contains("hello from bash"));
}

#[test]
fn run_bash_stderr_captured_to_error_file() {
    let dir = TempDir::new().unwrap();

    let pd = pipeline_dir(dir.path());
    setup_pipeline(
        dir.path(),
        r#"
version: 1
workspace: workspace
steps:
  - id: warn
    type: bash
    bash: echo "warning msg" >&2
    error: warnings.log
"#,
    );

    let cfg = Config::default();
    runner::run_pipeline(&pd, &cfg, false).unwrap();

    let workspace = pd.join("workspace");
    let content = fs::read_to_string(workspace.join("warnings.log")).unwrap();
    assert!(content.contains("warning msg"));
}

#[test]
fn run_void_output_discards_stdout() {
    let dir = TempDir::new().unwrap();

    let pd = pipeline_dir(dir.path());
    setup_pipeline(
        dir.path(),
        r#"
version: 1
workspace: workspace
steps:
  - id: noisy
    type: bash
    bash: echo "discard me"
    output: null
"#,
    );

    let cfg = Config::default();
    runner::run_pipeline(&pd, &cfg, false).unwrap();

    // Step should complete successfully, no output file created
    let s = state::load(&pd.join("state.json")).unwrap().unwrap();
    assert_eq!(s.steps["noisy"].status, StepStatus::Completed);
}

#[test]
fn run_default_output_no_file_created() {
    let dir = TempDir::new().unwrap();

    let pd = pipeline_dir(dir.path());
    setup_pipeline(
        dir.path(),
        r#"
version: 1
workspace: workspace
steps:
  - id: hello
    type: bash
    bash: echo "terminal output"
"#,
    );

    let cfg = Config::default();
    runner::run_pipeline(&pd, &cfg, false).unwrap();

    // No output/error files should be created in workspace
    let workspace = pd.join("workspace");
    let entries: Vec<_> = fs::read_dir(&workspace)
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert!(
        entries.is_empty(),
        "workspace should have no files, got: {:?}",
        entries
    );
}

#[test]
fn run_agent_missing_binary_gives_helpful_error() {
    let dir = TempDir::new().unwrap();

    let pd = pipeline_dir(dir.path());
    setup_pipeline(
        dir.path(),
        r#"
version: 1
workspace: workspace
steps:
  - id: analyse
    type: agent
    agent: worker
    prompt: "do work"
    output: result.md
"#,
    );

    let cfg = Config::default();

    // Point OPENCLAW_BIN at a nonexistent binary
    let fake_bin = dir.path().join("nonexistent-openclaw");
    let result = run_with_fake_openclaw(&pd, &fake_bin, &cfg);

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.contains("openclaw binary not found"),
        "expected helpful error, got: {}",
        err
    );
}
