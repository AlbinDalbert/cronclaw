use cronclaw::pipeline::{self, StepType};

// ─── Minimal valid pipelines ───

#[test]
fn parse_minimal_bash_step() {
    let yaml = r#"
version: 1
workspace: workspace
steps:
  - id: hello
    type: bash
    bash: echo "hello"
"#;
    let p = pipeline::parse(yaml).unwrap();
    assert_eq!(p.version, 1);
    assert_eq!(p.workspace, "workspace");
    assert_eq!(p.steps.len(), 1);
    assert_eq!(p.steps[0].id, "hello");
    assert_eq!(p.steps[0].step_type, StepType::Bash);
    assert_eq!(p.steps[0].bash.as_deref(), Some("echo \"hello\""));
    assert!(p.steps[0].outputs.is_empty());
    assert!(p.steps[0].timeout.is_none());
}

#[test]
fn parse_minimal_agent_step() {
    let yaml = r#"
version: 1
workspace: workspace
steps:
  - id: research
    type: agent
    agent: pro-worker
    prompt: Do some research.
"#;
    let p = pipeline::parse(yaml).unwrap();
    assert_eq!(p.steps[0].step_type, StepType::Agent);
    assert_eq!(p.steps[0].agent.as_deref(), Some("pro-worker"));
    assert_eq!(p.steps[0].prompt.as_deref(), Some("Do some research."));
}

// ─── Full-featured pipeline ───

#[test]
fn parse_full_pipeline() {
    let yaml = r#"
version: 1
workspace: workspace

steps:
  - id: wake
    type: bash
    bash: |
      wol-manager wake caladan

  - id: research
    type: agent
    agent: pro-worker
    timeout: 600
    prompt: |
      Research today's tech news.
      Write the result to summary.md.tmp.
    outputs:
      - name: summary
        path: summary.md
        tmp: summary.md.tmp

  - id: tts
    type: bash
    bash: |
      ./tts.sh summary.md.tmp audio.wav.tmp
    outputs:
      - name: audio
        path: audio.wav
        tmp: audio.wav.tmp
"#;
    let p = pipeline::parse(yaml).unwrap();
    assert_eq!(p.steps.len(), 3);

    // wake
    assert_eq!(p.steps[0].id, "wake");
    assert_eq!(p.steps[0].step_type, StepType::Bash);
    assert!(p.steps[0].bash.as_ref().unwrap().contains("wol-manager"));
    assert!(p.steps[0].timeout.is_none());

    // research
    assert_eq!(p.steps[1].id, "research");
    assert_eq!(p.steps[1].step_type, StepType::Agent);
    assert_eq!(p.steps[1].timeout, Some(600));
    assert_eq!(p.steps[1].outputs.len(), 1);
    assert_eq!(p.steps[1].outputs[0].name, "summary");
    assert_eq!(p.steps[1].outputs[0].path, "summary.md");
    assert_eq!(p.steps[1].outputs[0].tmp, "summary.md.tmp");

    // tts
    assert_eq!(p.steps[2].outputs.len(), 1);
    assert_eq!(p.steps[2].outputs[0].name, "audio");
}

// ─── Multiline strings ───

#[test]
fn parse_multiline_bash() {
    let yaml = r#"
version: 1
workspace: workspace
steps:
  - id: multi
    type: bash
    bash: |
      echo "line 1"
      echo "line 2"
      echo "line 3"
"#;
    let p = pipeline::parse(yaml).unwrap();
    let script = p.steps[0].bash.as_ref().unwrap();
    assert!(script.contains("line 1"));
    assert!(script.contains("line 2"));
    assert!(script.contains("line 3"));
}

#[test]
fn parse_multiline_prompt() {
    let yaml = r#"
version: 1
workspace: workspace
steps:
  - id: agent-step
    type: agent
    agent: worker
    prompt: |
      First line of prompt.
      Second line of prompt.

      Fourth line after blank.
"#;
    let p = pipeline::parse(yaml).unwrap();
    let prompt = p.steps[0].prompt.as_ref().unwrap();
    assert!(prompt.contains("First line"));
    assert!(prompt.contains("Second line"));
    assert!(prompt.contains("Fourth line"));
}

// ─── Multiple outputs ───

#[test]
fn parse_multiple_outputs() {
    let yaml = r#"
version: 1
workspace: workspace
steps:
  - id: gen
    type: bash
    bash: ./generate.sh
    outputs:
      - name: text
        path: result.txt
        tmp: result.txt.tmp
      - name: data
        path: result.json
        tmp: result.json.tmp
"#;
    let p = pipeline::parse(yaml).unwrap();
    assert_eq!(p.steps[0].outputs.len(), 2);
    assert_eq!(p.steps[0].outputs[0].name, "text");
    assert_eq!(p.steps[0].outputs[1].name, "data");
}

// ─── Per-step timeout ───

#[test]
fn parse_step_timeout() {
    let yaml = r#"
version: 1
workspace: workspace
steps:
  - id: quick
    type: bash
    bash: echo fast
  - id: slow
    type: bash
    timeout: 3600
    bash: ./long-running.sh
"#;
    let p = pipeline::parse(yaml).unwrap();
    assert!(p.steps[0].timeout.is_none());
    assert_eq!(p.steps[1].timeout, Some(3600));
}

// ─── Validation failures ───

#[test]
fn reject_bash_step_missing_bash_field() {
    let yaml = r#"
version: 1
workspace: workspace
steps:
  - id: broken
    type: bash
"#;
    let err = pipeline::parse(yaml).unwrap_err();
    assert!(err.contains("broken"));
    assert!(err.contains("bash"));
}

#[test]
fn reject_agent_step_missing_agent_field() {
    let yaml = r#"
version: 1
workspace: workspace
steps:
  - id: broken
    type: agent
    prompt: Do something.
"#;
    let err = pipeline::parse(yaml).unwrap_err();
    assert!(err.contains("broken"));
    assert!(err.contains("agent"));
}

#[test]
fn reject_agent_step_missing_prompt_field() {
    let yaml = r#"
version: 1
workspace: workspace
steps:
  - id: broken
    type: agent
    agent: worker
"#;
    let err = pipeline::parse(yaml).unwrap_err();
    assert!(err.contains("broken"));
    assert!(err.contains("prompt"));
}

#[test]
fn reject_unknown_step_type() {
    let yaml = r#"
version: 1
workspace: workspace
steps:
  - id: broken
    type: docker
    bash: echo hi
"#;
    assert!(pipeline::parse(yaml).is_err());
}

#[test]
fn reject_missing_version() {
    let yaml = r#"
workspace: workspace
steps:
  - id: hello
    type: bash
    bash: echo hi
"#;
    assert!(pipeline::parse(yaml).is_err());
}

#[test]
fn reject_missing_workspace() {
    let yaml = r#"
version: 1
steps:
  - id: hello
    type: bash
    bash: echo hi
"#;
    assert!(pipeline::parse(yaml).is_err());
}

#[test]
fn reject_missing_steps() {
    let yaml = r#"
version: 1
workspace: workspace
"#;
    assert!(pipeline::parse(yaml).is_err());
}

#[test]
fn reject_step_missing_id() {
    let yaml = r#"
version: 1
workspace: workspace
steps:
  - type: bash
    bash: echo hi
"#;
    assert!(pipeline::parse(yaml).is_err());
}

#[test]
fn reject_step_missing_type() {
    let yaml = r#"
version: 1
workspace: workspace
steps:
  - id: hello
    bash: echo hi
"#;
    assert!(pipeline::parse(yaml).is_err());
}

#[test]
fn reject_empty_steps_array() {
    // Empty steps should parse (it's a valid Vec), but this tests the schema allows it.
    // Whether the runner handles it gracefully is a separate concern.
    let yaml = r#"
version: 1
workspace: workspace
steps: []
"#;
    let p = pipeline::parse(yaml).unwrap();
    assert!(p.steps.is_empty());
}

// ─── Optional fields don't interfere ───

#[test]
fn bash_step_ignores_agent_fields() {
    let yaml = r#"
version: 1
workspace: workspace
steps:
  - id: hello
    type: bash
    bash: echo hi
    agent: should-be-ignored
    prompt: also-ignored
"#;
    let p = pipeline::parse(yaml).unwrap();
    assert_eq!(p.steps[0].step_type, StepType::Bash);
    assert_eq!(p.steps[0].bash.as_deref(), Some("echo hi"));
    // Extra fields are parsed but not validated for bash type
    assert_eq!(p.steps[0].agent.as_deref(), Some("should-be-ignored"));
}

#[test]
fn outputs_default_to_empty() {
    let yaml = r#"
version: 1
workspace: workspace
steps:
  - id: hello
    type: bash
    bash: echo hi
"#;
    let p = pipeline::parse(yaml).unwrap();
    assert!(p.steps[0].outputs.is_empty());
}
