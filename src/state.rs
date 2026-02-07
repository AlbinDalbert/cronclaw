use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::pipeline::Pipeline;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum StepStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StepState {
    pub status: StepStatus,
}

/// Ordered map of step id -> step state.
/// BTreeMap keeps keys sorted, but we rely on pipeline.yaml for ordering
/// and just use this for lookup.
#[derive(Debug, Serialize, Deserialize)]
pub struct State {
    pub steps: BTreeMap<String, StepState>,
}

impl State {
    pub fn from_pipeline(pipeline: &Pipeline) -> Self {
        let mut steps = BTreeMap::new();
        for step in &pipeline.steps {
            steps.insert(
                step.id.clone(),
                StepState {
                    status: StepStatus::Pending,
                },
            );
        }
        State { steps }
    }
}

pub fn load(path: &Path) -> Result<Option<State>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(path)
        .map_err(|e| format!("failed to read state: {}", e))?;
    let state: State = serde_json::from_str(&content)
        .map_err(|e| format!("failed to parse state: {}", e))?;
    Ok(Some(state))
}

pub fn save(path: &Path, state: &State) -> Result<(), String> {
    let content = serde_json::to_string_pretty(state)
        .map_err(|e| format!("failed to serialize state: {}", e))?;
    fs::write(path, content)
        .map_err(|e| format!("failed to write state: {}", e))?;
    Ok(())
}
