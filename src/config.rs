use serde::Deserialize;
use std::fs;
use std::path::Path;

const DEFAULT_TIMEOUT: u64 = 300; // 5 minutes

#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(default = "default_timeout")]
    pub timeout: u64,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            timeout: DEFAULT_TIMEOUT,
        }
    }
}

fn default_timeout() -> u64 {
    DEFAULT_TIMEOUT
}

pub fn load(path: &Path) -> Config {
    if !path.exists() {
        return Config::default();
    }

    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Config::default(),
    };

    serde_yaml::from_str(&content).unwrap_or_default()
}

