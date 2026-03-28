use serde::{Deserialize, Serialize};
use anyhow::Result;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AspConfig {
    pub project: Project,
    #[serde(default)]
    pub component: Vec<Component>,
    #[serde(default)]
    pub rule: Vec<Rule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub name: String,
    pub languages: Vec<String>,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Component {
    pub name: String,
    pub description: Option<String>,
    pub paths: Vec<String>,
    pub owner: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub name: String,
    #[serde(rename = "type")]
    pub rule_type: String,
    #[serde(flatten)]
    pub config: std::collections::HashMap<String, toml::Value>,
}

impl AspConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: AspConfig = toml::from_str(&content)?;
        Ok(config)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }
}

impl Default for AspConfig {
    fn default() -> Self {
        Self {
            project: Project {
                name: "unnamed".to_string(),
                languages: vec![],
                version: "1".to_string(),
            },
            component: vec![],
            rule: vec![],
        }
    }
}
