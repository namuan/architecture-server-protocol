use anyhow::Result;
use clap::{Parser, Subcommand};
use sha2::{Sha256, Digest};
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "asp", about = "Architecture Server Protocol CLI", version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Walk the project, build index, call LLM, write architecture.toml
    Init {
        #[arg(long, default_value = ".")]
        project_root: PathBuf,
        #[arg(long)]
        yes: bool,
    },
    /// Print component dependency graph
    Graph {
        #[arg(long, default_value = "text")]
        format: String,
    },
    /// Explain a file or component with streaming LLM output
    Explain {
        path: String,
    },
    /// Run rule engine
    Check {
        #[arg(long)]
        rule: Option<String>,
        #[arg(long, default_value = "text")]
        format: String,
    },
    /// Start file watcher
    Watch {
        #[arg(long, default_value = "text")]
        format: String,
    },
    /// Re-run LLM inference on dirty components
    Refresh {
        #[arg(long)]
        force: bool,
    },
    /// Print current state of the index
    Status,
}

#[derive(serde::Deserialize, Default)]
#[allow(dead_code)]
struct GlobalConfig {
    #[serde(default)]
    llm: LlmConfig,
    #[serde(default)]
    staleness: StalenessConfig,
}

#[derive(serde::Deserialize)]
struct LlmConfig {
    base_url: String,
    model: String,
    api_key: String,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:11434/v1".to_string(),
            model: "llama3.1:8b".to_string(),
            api_key: String::new(),
        }
    }
}

#[derive(serde::Deserialize)]
#[allow(dead_code)]
struct StalenessConfig {
    threshold: f64,
}

impl Default for StalenessConfig {
    fn default() -> Self {
        Self { threshold: 0.15 }
    }
}

fn load_global_config() -> GlobalConfig {
    let config_path = dirs_home().join(".asp").join("config.toml");
    if config_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&config_path) {
            if let Ok(cfg) = toml::from_str(&content) {
                return cfg;
            }
        }
    }
    GlobalConfig::default()
}

fn dirs_home() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

fn project_db_path(project_root: &PathBuf) -> PathBuf {
    let canonical = project_root.canonicalize().unwrap_or_else(|_| project_root.clone());
    let path_str = canonical.to_string_lossy();
    let mut hasher = Sha256::new();
    hasher.update(path_str.as_bytes());
    let hash = format!("{:x}", hasher.finalize());
    let short_hash = &hash[..12];
    dirs_home().join(".asp").join(format!("{}.db", short_hash))
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    let config = load_global_config();

    match cli.command {
        Commands::Init { project_root, yes } => {
            cmd_init(project_root, yes, &config).await?;
        }
        Commands::Graph { format } => {
            cmd_graph(&format)?;
        }
        Commands::Explain { path } => {
            cmd_explain(&path, &config).await?;
        }
        Commands::Check { rule, format } => {
            cmd_check(rule.as_deref(), &format)?;
        }
        Commands::Watch { format } => {
            cmd_watch(&format)?;
        }
        Commands::Refresh { force } => {
            cmd_refresh(force, &config).await?;
        }
        Commands::Status => {
            cmd_status()?;
        }
    }

    Ok(())
}

async fn cmd_init(project_root: PathBuf, _yes: bool, _config: &GlobalConfig) -> Result<()> {
    println!("Initializing ASP for {:?}", project_root);

    std::fs::create_dir_all(dirs_home().join(".asp"))?;
    let db_path = project_db_path(&project_root);
    let db = asp_db::Database::open(db_path.to_str().unwrap_or(":memory:"))?;

    let indexer = asp_core::Indexer::new(db, project_root.clone());
    indexer.index_directory(&project_root)?;

    let arch_toml = project_root.join("architecture.toml");
    if !arch_toml.exists() {
        let default_config = asp_core::AspConfig::default();
        default_config.save(&arch_toml)?;
        println!("Created architecture.toml");
    } else {
        println!("architecture.toml already exists");
    }

    println!("Done.");
    Ok(())
}

fn cmd_graph(format: &str) -> Result<()> {
    let project_root = PathBuf::from(".");
    let db_path = project_db_path(&project_root);

    if !db_path.exists() {
        println!("No index found. Run `asp init` first.");
        return Ok(());
    }

    let db = asp_db::Database::open(db_path.to_str().unwrap_or(":memory:"))?;
    let resolver = asp_core::Resolver::new(&db);
    let graph = resolver.component_dependency_graph()?;

    match format {
        "dot" => {
            println!("digraph G {{");
            for (from, tos) in &graph {
                for to in tos {
                    println!("  \"{}\" -> \"{}\";", from, to);
                }
            }
            println!("}}");
        }
        "json" => {
            println!("{}", serde_json::to_string_pretty(&graph)?);
        }
        _ => {
            for (from, tos) in &graph {
                for to in tos {
                    println!("{} -> {}", from, to);
                }
            }
        }
    }

    Ok(())
}

async fn cmd_explain(path: &str, config: &GlobalConfig) -> Result<()> {
    use asp_llm::{LlmClient, OpenAiCompatClient, CompletionRequest, Message, Role};
    use futures::StreamExt;

    let client = OpenAiCompatClient::new(
        config.llm.base_url.clone(),
        config.llm.model.clone(),
        config.llm.api_key.clone(),
    );

    let req = CompletionRequest {
        messages: vec![
            Message { role: Role::System, content: "You are a code architecture assistant.".to_string() },
            Message { role: Role::User, content: format!("Explain the architecture role of: {}", path) },
        ],
        max_tokens: Some(512),
        temperature: Some(0.7),
    };

    let mut stream = client.stream(req).await?;
    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(text) => print!("{}", text),
            Err(e) => eprintln!("\nStream error: {}", e),
        }
    }
    println!();

    Ok(())
}

fn cmd_check(rule_filter: Option<&str>, format: &str) -> Result<()> {
    let project_root = PathBuf::from(".");
    let db_path = project_db_path(&project_root);

    if !db_path.exists() {
        println!("No index found. Run `asp init` first.");
        return Ok(());
    }

    let arch_toml = project_root.join("architecture.toml");
    let config = if arch_toml.exists() {
        asp_core::AspConfig::load(&arch_toml)?
    } else {
        asp_core::AspConfig::default()
    };

    let db = asp_db::Database::open(db_path.to_str().unwrap_or(":memory:"))?;
    let engine = asp_rules::RuleEngine::new(&db, &config);
    let mut violations = engine.check_all()?;

    if let Some(rule) = rule_filter {
        violations.retain(|v| v.rule == rule);
    }

    match format {
        "json" => println!("{}", serde_json::to_string_pretty(&violations)?),
        _ => {
            if violations.is_empty() {
                println!("No violations found.");
            } else {
                for v in &violations {
                    println!("[{:?}] {}: {}", v.severity, v.rule, v.message);
                }
            }
        }
    }

    Ok(())
}

fn cmd_watch(format: &str) -> Result<()> {
    use asp_watcher::FileWatcher;
    use std::collections::HashSet;
    use std::path::PathBuf as PB;

    let project_root = PathBuf::from(".");
    let db_path = project_db_path(&project_root);
    let watcher = FileWatcher::new(project_root.clone());

    println!("Watching for changes... (Ctrl+C to stop)");

    watcher.watch(move |changed: HashSet<PB>| {
        let db = asp_db::Database::open(db_path.to_str().unwrap_or(":memory:"))?;
        let indexer = asp_core::Indexer::new(db, project_root.clone());

        for path in &changed {
            match format {
                "json" => println!("{}", serde_json::json!({"changed": path.to_string_lossy()})),
                _ => println!("Changed: {}", path.display()),
            }
            if let Err(e) = indexer.index_file(path) {
                eprintln!("Index error: {}", e);
            }
        }
        Ok(())
    })?;

    Ok(())
}

async fn cmd_refresh(_force: bool, _config: &GlobalConfig) -> Result<()> {
    println!("Refreshing LLM metadata for dirty components...");
    println!("Done (stub).");
    Ok(())
}

fn cmd_status() -> Result<()> {
    let project_root = PathBuf::from(".");
    let db_path = project_db_path(&project_root);

    if !db_path.exists() {
        println!("No index found. Run `asp init` first.");
        return Ok(());
    }

    let db = asp_db::Database::open(db_path.to_str().unwrap_or(":memory:"))?;

    let file_count: i64 = db.conn.query_row(
        "SELECT COUNT(*) FROM files", [], |row| row.get(0)
    ).unwrap_or(0);

    let import_count: i64 = db.conn.query_row(
        "SELECT COUNT(*) FROM imports", [], |row| row.get(0)
    ).unwrap_or(0);

    let symbol_count: i64 = db.conn.query_row(
        "SELECT COUNT(*) FROM symbols", [], |row| row.get(0)
    ).unwrap_or(0);

    println!("ASP Status:");
    println!("  Files indexed:  {}", file_count);
    println!("  Imports:        {}", import_count);
    println!("  Symbols:        {}", symbol_count);
    println!("  DB:             {}", db_path.display());

    Ok(())
}
