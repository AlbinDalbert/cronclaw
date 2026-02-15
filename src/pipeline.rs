use serde::Deserialize;
use std::fs;
use std::path::Path;

/// Where to route a stream (stdout or stderr) from a step.
///
/// - Missing from YAML → `Terminal` (print to terminal)
/// - `output: null`    → `Void` (discard)
/// - `output: path`    → `File(path)` (write to file in workspace)
#[derive(Debug, Clone, PartialEq)]
pub enum StreamTarget {
    Terminal,
    Void,
    File(String),
}

impl Default for StreamTarget {
    fn default() -> Self {
        StreamTarget::Terminal
    }
}

impl<'de> Deserialize<'de> for StreamTarget {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let opt: Option<String> = Option::deserialize(deserializer)?;
        match opt {
            None => Ok(StreamTarget::Void),
            Some(s) => Ok(StreamTarget::File(s)),
        }
    }
}

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

    // Stream routing (shared across step types)
    #[serde(default)]
    pub output: StreamTarget,
    #[serde(default)]
    pub error: StreamTarget,

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

pub fn parse(content: &str) -> Result<Pipeline, String> {
    let pipeline: Pipeline =
        serde_yaml::from_str(content).map_err(|e| format!("failed to parse pipeline: {}", e))?;

    for step in &pipeline.steps {
        match step.step_type {
            StepType::Bash => {
                if step.bash.is_none() {
                    return Err(format!(
                        "step '{}': type is bash but 'bash' field is missing",
                        step.id
                    ));
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

pub fn load(path: &Path) -> Result<Pipeline, String> {
    let content = fs::read_to_string(path)
        .map_err(|e| format!("failed to read {}: {}", path.display(), e))?;
    parse(&content).map_err(|e| format!("{}: {}", path.display(), e))
}
