use cronclaw::config::Config;
use cronclaw::pipeline;
use cronclaw::runner;
use cronclaw::state::{self, State, StepStatus};
use std::fs;
use tempfile::TempDir;

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
    assert_eq!(fs::read_to_string(dir.path().join("out.txt")).unwrap(), "data");
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
