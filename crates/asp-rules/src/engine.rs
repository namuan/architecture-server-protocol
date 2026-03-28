use anyhow::Result;
use asp_core::{AspConfig, Resolver};
use asp_db::Database;
use serde::{Deserialize, Serialize};
use crate::cycles::TarjanScc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Violation {
    pub rule: String,
    pub severity: Severity,
    pub message: String,
    pub file: Option<String>,
    pub component: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

pub struct RuleEngine<'a> {
    pub db: &'a Database,
    pub config: &'a AspConfig,
}

impl<'a> RuleEngine<'a> {
    pub fn new(db: &'a Database, config: &'a AspConfig) -> Self {
        Self { db, config }
    }

    pub fn check_all(&self) -> Result<Vec<Violation>> {
        let mut violations = Vec::new();
        let resolver = Resolver::new(self.db);

        for rule in &self.config.rule {
            match rule.rule_type.as_str() {
                "no_cycles" => {
                    let graph = resolver.component_dependency_graph()?;
                    let cycles = TarjanScc::new(graph).find_cycles();
                    for cycle in cycles {
                        violations.push(Violation {
                            rule: rule.name.clone(),
                            severity: Severity::Error,
                            message: format!("Circular dependency detected: {}", cycle.join(" -> ")),
                            file: None,
                            component: None,
                        });
                    }
                }
                "no_unowned" => {
                    let mut stmt = self.db.conn.prepare(
                        "SELECT path FROM files WHERE component IS NULL"
                    )?;
                    let paths: Vec<String> = stmt.query_map([], |row| row.get(0))?
                        .filter_map(|r| r.ok())
                        .collect();
                    for path in paths {
                        violations.push(Violation {
                            rule: rule.name.clone(),
                            severity: Severity::Warning,
                            message: format!("File has no owning component: {}", path),
                            file: Some(path),
                            component: None,
                        });
                    }
                }
                "deny_import" => {
                    // stub
                }
                "max_fan_in" => {
                    // stub
                }
                _ => {
                    tracing::warn!("Unknown rule type: {}", rule.rule_type);
                }
            }
        }

        Ok(violations)
    }
}
