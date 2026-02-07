use cronclaw::pipeline;
use cronclaw::state::{self, State, StepStatus};
use std::fs;
use tempfile::TempDir;

#[test]
fn state_from_pipeline_all_pending() {
    let yaml = r#"
version: 1
workspace: workspace
steps:
  - id: step-a
    type: bash
    bash: echo a
  - id: step-b
    type: bash
    bash: echo b
  - id: step-c
    type: bash
    bash: echo c
"#;
    let p = pipeline::parse(yaml).unwrap();
    let s = State::from_pipeline(&p);

    assert_eq!(s.steps.len(), 3);
    for (_, step_state) in &s.steps {
        assert_eq!(step_state.status, StepStatus::Pending);
    }
    assert!(s.steps.contains_key("step-a"));
    assert!(s.steps.contains_key("step-b"));
    assert!(s.steps.contains_key("step-c"));
}

#[test]
fn state_roundtrip_json() {
    let yaml = r#"
version: 1
workspace: workspace
steps:
  - id: first
    type: bash
    bash: echo 1
  - id: second
    type: bash
    bash: echo 2
"#;
    let p = pipeline::parse(yaml).unwrap();
    let mut s = State::from_pipeline(&p);

    // Modify some states
    s.steps.get_mut("first").unwrap().status = StepStatus::Completed;
    s.steps.get_mut("second").unwrap().status = StepStatus::Running;

    let dir = TempDir::new().unwrap();
    let path = dir.path().join("state.json");

    state::save(&path, &s).unwrap();
    let loaded = state::load(&path).unwrap().unwrap();

    assert_eq!(loaded.steps["first"].status, StepStatus::Completed);
    assert_eq!(loaded.steps["second"].status, StepStatus::Running);
}

#[test]
fn state_load_nonexistent_returns_none() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("does-not-exist.json");
    let result = state::load(&path).unwrap();
    assert!(result.is_none());
}

#[test]
fn state_json_uses_lowercase_status_strings() {
    let yaml = r#"
version: 1
workspace: workspace
steps:
  - id: a
    type: bash
    bash: echo a
"#;
    let p = pipeline::parse(yaml).unwrap();
    let mut s = State::from_pipeline(&p);
    s.steps.get_mut("a").unwrap().status = StepStatus::Failed;

    let dir = TempDir::new().unwrap();
    let path = dir.path().join("state.json");
    state::save(&path, &s).unwrap();

    let raw = fs::read_to_string(&path).unwrap();
    assert!(raw.contains("\"failed\""));
    assert!(!raw.contains("\"Failed\""));
}

#[test]
fn state_all_status_variants_serialize() {
    let yaml = r#"
version: 1
workspace: workspace
steps:
  - id: a
    type: bash
    bash: echo a
  - id: b
    type: bash
    bash: echo b
  - id: c
    type: bash
    bash: echo c
  - id: d
    type: bash
    bash: echo d
"#;
    let p = pipeline::parse(yaml).unwrap();
    let mut s = State::from_pipeline(&p);
    s.steps.get_mut("a").unwrap().status = StepStatus::Pending;
    s.steps.get_mut("b").unwrap().status = StepStatus::Running;
    s.steps.get_mut("c").unwrap().status = StepStatus::Completed;
    s.steps.get_mut("d").unwrap().status = StepStatus::Failed;

    let dir = TempDir::new().unwrap();
    let path = dir.path().join("state.json");
    state::save(&path, &s).unwrap();
    let loaded = state::load(&path).unwrap().unwrap();

    assert_eq!(loaded.steps["a"].status, StepStatus::Pending);
    assert_eq!(loaded.steps["b"].status, StepStatus::Running);
    assert_eq!(loaded.steps["c"].status, StepStatus::Completed);
    assert_eq!(loaded.steps["d"].status, StepStatus::Failed);
}
