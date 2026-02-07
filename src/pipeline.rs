use serde::Deserialize;
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct Pipeline {
    pub version: u32,
    pub workspace: String,
    pub steps: Vec<Step>,
}

#[derive(Debug, Deserialize)]
pub struct Step {
    pub id: String,
    #[serde(rename = "type")]
    pub step_type: StepType,

    // Agent fields
    pub agent: Option<String>,
    pub prompt: Option<String>,

    // Bash fields
    pub bash: Option<String>,

    // Per-step timeout override (seconds)
    pub timeout: Option<u64>,

    // Outputs
    #[serde(default)]
    pub outputs: Vec<Output>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum StepType {
    Agent,
    Bash,
}

#[derive(Debug, Deserialize)]
pub struct Output {
    pub name: String,
    pub path: String,
    pub tmp: String,
}

pub fn load(path: &Path) -> Result<Pipeline, String> {
    let content = fs::read_to_string(path)
        .map_err(|e| format!("failed to read {}: {}", path.display(), e))?;
    let pipeline: Pipeline = serde_yaml::from_str(&content)
        .map_err(|e| format!("failed to parse {}: {}", path.display(), e))?;

    for step in &pipeline.steps {
        match step.step_type {
            StepType::Bash => {
                if step.bash.is_none() {
                    return Err(format!("step '{}': type is bash but 'bash' field is missing", step.id));
                }
            }
            StepType::Agent => {
                if step.agent.is_none() || step.prompt.is_none() {
                    return Err(format!(
                        "step '{}': type is agent but 'agent' or 'prompt' field is missing",
                        step.id
                    ));
                }
            }
        }
    }

    Ok(pipeline)
}
