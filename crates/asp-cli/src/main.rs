use anyhow::Result;
use clap::{Parser, Subcommand};
use sha2::{Sha256, Digest};
use std::path::{Path, PathBuf};
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
            base_url: "http://127.0.0.1:8080/v1".to_string(),
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

async fn cmd_init(project_root: PathBuf, yes: bool, config: &GlobalConfig) -> Result<()> {
    println!("Initializing ASP for {:?}", project_root);

    std::fs::create_dir_all(dirs_home().join(".asp"))?;
    let db_path = project_db_path(&project_root);
    let db = asp_db::Database::open(db_path.to_str().unwrap_or(":memory:"))?;

    let indexer = asp_core::Indexer::new(db, project_root.clone());
    indexer.index_directory(&project_root)?;

    let arch_toml = project_root.join("architecture.toml");
    if !arch_toml.exists() {
        let asp_config = scaffold_config(&project_root);
        let n = asp_config.component.len();
        asp_config.save(&arch_toml)?;
        if n > 0 {
            println!("Created architecture.toml with {} component(s)", n);
        } else {
            println!("Created architecture.toml (no components detected)");
        }
    } else {
        println!("architecture.toml already exists");
    }

    if yes {
        println!("Generating LLM descriptions for components (--yes)...");
        run_refresh(&project_root, false, config).await?;
    }

    println!("Done.");
    Ok(())
}

/// Build a starter `AspConfig` by inspecting the directory tree:
/// - project name from `Cargo.toml` / `package.json` / directory basename
/// - languages from file extensions actually present
/// - components from meaningful subdirectory boundaries
fn scaffold_config(project_root: &Path) -> asp_core::AspConfig {
    let name = detect_project_name(project_root);
    let languages = detect_languages(project_root);
    let components = discover_components(project_root);

    asp_core::AspConfig {
        project: asp_core::Project {
            name,
            languages,
            version: "1".to_string(),
        },
        component: components,
        rule: vec![
            asp_core::Rule {
                name: "no_cycles".to_string(),
                rule_type: "no_cycles".to_string(),
                config: Default::default(),
            },
            asp_core::Rule {
                name: "no_unowned".to_string(),
                rule_type: "no_unowned".to_string(),
                config: Default::default(),
            },
        ],
    }
}

/// Try Cargo.toml `[package] name`, then package.json `name`, then the directory basename.
fn detect_project_name(root: &Path) -> String {
    // Cargo.toml
    if let Ok(content) = std::fs::read_to_string(root.join("Cargo.toml")) {
        if let Ok(val) = content.parse::<toml::Value>() {
            if let Some(name) = val.get("package").and_then(|p| p.get("name")).and_then(|n| n.as_str()) {
                return name.to_string();
            }
        }
    }
    // package.json
    if let Ok(content) = std::fs::read_to_string(root.join("package.json")) {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(name) = val.get("name").and_then(|n| n.as_str()) {
                return name.to_string();
            }
        }
    }
    // fallback: directory basename (canonicalize so "." resolves to the real name)
    let canonical = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    canonical
        .file_name()
        .map(|n: &std::ffi::OsStr| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "unnamed".to_string())
}

/// Walk the tree and collect language names from file extensions.
fn detect_languages(root: &Path) -> Vec<String> {
    let mut found = std::collections::HashSet::new();
    for entry in ignore::Walk::new(root).flatten() {
        let entry_path = entry.path().to_path_buf();
        if !entry_path.is_file() {
            continue;
        }
        match entry_path.extension().and_then(|e| e.to_str()) {
            Some("py")              => { found.insert("python"); }
            Some("js") | Some("jsx") | Some("mjs") | Some("cjs") => { found.insert("javascript"); }
            Some("ts") | Some("tsx") => { found.insert("typescript"); }
            Some("rs")              => { found.insert("rust"); }
            Some("go")              => { found.insert("go"); }
            Some("java")            => { found.insert("java"); }
            Some("rb")              => { found.insert("ruby"); }
            _ => {}
        }
    }
    // Stable ordering
    let mut langs: Vec<String> = found.into_iter().map(|s| s.to_string()).collect();
    langs.sort();
    langs
}

// Directories that are build artifacts, tooling, or not logical components.
const SKIP_DIRS: &[&str] = &[
    "target", "node_modules", ".git", ".github", "dist", "build",
    "out", "__pycache__", ".venv", "venv", ".tox", "coverage",
];

/// Infer component directories from the project layout.
///
/// Strategy:
/// 1. If a workspace-style container exists (`crates/`, `packages/`, `apps/`, `libs/`),
///    its immediate children become components.
/// 2. Otherwise, immediate non-skipped subdirectories of the project root become components.
fn discover_components(root: &Path) -> Vec<asp_core::Component> {
    // Container directories that conventionally hold multiple sub-packages.
    const CONTAINERS: &[&str] = &["crates", "packages", "apps", "libs", "modules", "services", "src"];

    let mut component_dirs: Vec<(String, String)> = Vec::new(); // (name, glob_pattern)

    // Check for workspace containers first.
    for container in CONTAINERS {
        let container_path = root.join(container);
        if !container_path.is_dir() {
            continue;
        }
        let children = subdir_children(&container_path);
        if children.is_empty() {
            continue;
        }
        // "src" is only treated as a container when it has meaningful subdirectories
        // (i.e. it's not itself the sole source directory).
        if *container == "src" && children.len() <= 1 {
            continue;
        }
        for child_name in children {
            component_dirs.push((
                child_name.clone(),
                format!("{}/{}/**", container, child_name),
            ));
        }
        // Found a container — don't also add top-level dirs.
        if !component_dirs.is_empty() {
            break;
        }
    }

    // Fall back to top-level subdirectories.
    if component_dirs.is_empty() {
        for child_name in subdir_children(root) {
            if CONTAINERS.contains(&child_name.as_str()) {
                continue; // already considered above (but was empty)
            }
            component_dirs.push((
                child_name.clone(),
                format!("{}/**", child_name),
            ));
        }
    }

    component_dirs
        .into_iter()
        .map(|(name, pattern)| asp_core::Component {
            name,
            description: None,
            paths: vec![pattern],
            owner: None,
        })
        .collect()
}

/// Return names of immediate subdirectories of `dir` that are not in `SKIP_DIRS`
/// and don't start with `.`.
fn subdir_children(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else { return vec![] };
    let mut names: Vec<String> = entries
        .flatten()
        .filter(|e| e.file_type().map_or(false, |t| t.is_dir()))
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| !n.starts_with('.') && !SKIP_DIRS.contains(&n.as_str()))
        .collect();
    names.sort();
    names
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

    // Enrich the prompt with indexed context when the DB is available.
    let project_root = PathBuf::from(".");
    let db_path = project_db_path(&project_root);
    let db_context = if db_path.exists() {
        asp_db::Database::open(db_path.to_str().unwrap_or(":memory:"))
            .ok()
            .map(|db| build_file_context(&db, path))
            .unwrap_or_default()
    } else {
        String::new()
    };

    let user_content = if db_context.is_empty() {
        format!("Explain the architectural role of: {}", path)
    } else {
        format!("Explain the architectural role of: {}\n\nIndexed context:\n{}", path, db_context)
    };

    let client = OpenAiCompatClient::new(
        config.llm.base_url.clone(),
        config.llm.model.clone(),
        config.llm.api_key.clone(),
    );

    let req = CompletionRequest {
        messages: vec![
            Message { role: Role::System, content: "You are a code architecture assistant.".to_string() },
            Message { role: Role::User, content: user_content },
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

const MAX_CONTEXT_SYMBOLS: usize = 15;
const MAX_CONTEXT_IMPORTS: usize = 10;

/// Strip `root` prefix from `path` and remove the leading `/`, returning a relative path.
fn strip_root<'a>(path: &'a str, root: &str) -> &'a str {
    path.strip_prefix(root)
        .map(|s| s.trim_start_matches('/'))
        .unwrap_or(path)
}

/// Build a context string for a single file from the DB (component, AI description, symbols, imports).
fn build_file_context(db: &asp_db::Database, path: &str) -> String {
    let lookup = PathBuf::from(path)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(path))
        .to_string_lossy()
        .into_owned();
    let mut lines: Vec<String> = Vec::new();

    let component: Option<String> = db.conn
        .query_row("SELECT component FROM files WHERE path = ?1", [&lookup], |r| r.get(0))
        .ok()
        .flatten();
    if let Some(ref c) = component {
        lines.push(format!("Component: {}", c));
        if let Some(rationale) = db.conn
            .query_row("SELECT rationale FROM ai_metadata WHERE component = ?1", [c], |r| r.get::<_, String>(0))
            .ok()
        {
            lines.push(format!("Component description: {}", rationale));
        }
    }

    if let Ok(mut stmt) = db.conn.prepare(
        "SELECT name, kind FROM symbols WHERE file = ?1 ORDER BY kind, name LIMIT ?2"
    ) {
        let syms: Vec<String> = stmt
            .query_map(rusqlite::params![lookup, MAX_CONTEXT_SYMBOLS as i64], |row| {
                Ok(format!("{} ({})", row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default();
        if !syms.is_empty() {
            lines.push(format!("Symbols: {}", syms.join(", ")));
        }
    }

    if let Ok(mut stmt) = db.conn.prepare(
        "SELECT to_raw FROM imports WHERE from_file = ?1 LIMIT ?2"
    ) {
        let imps: Vec<String> = stmt
            .query_map(rusqlite::params![lookup, MAX_CONTEXT_IMPORTS as i64], |row| row.get(0))
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default();
        if !imps.is_empty() {
            lines.push(format!("Imports: {}", imps.join("; ")));
        }
    }

    lines.join("\n")
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

async fn cmd_refresh(force: bool, config: &GlobalConfig) -> Result<()> {
    let project_root = PathBuf::from(".");
    run_refresh(&project_root, force, config).await
}

/// Core refresh logic, shared by `cmd_refresh` and `cmd_init --yes`.
async fn run_refresh(project_root: &Path, force: bool, config: &GlobalConfig) -> Result<()> {
    use asp_llm::{LlmClient, OpenAiCompatClient, CompletionRequest, Message, Role};

    let db_path = project_db_path(&project_root.to_path_buf());
    if !db_path.exists() {
        println!("No index found. Run `asp init` first.");
        return Ok(());
    }

    let arch_toml = project_root.join("architecture.toml");
    if !arch_toml.exists() {
        println!("No architecture.toml found. Run `asp init` first.");
        return Ok(());
    }

    let asp_config = asp_core::AspConfig::load(&arch_toml)?;
    if asp_config.component.is_empty() {
        println!("No components defined in architecture.toml.");
        return Ok(());
    }

    let db = asp_db::Database::open(db_path.to_str().unwrap_or(":memory:"))?;
    let root_str = project_root.canonicalize()
        .unwrap_or_else(|_| project_root.to_path_buf())
        .to_string_lossy()
        .into_owned();

    // Phase 1: build requests synchronously (DB access is not Send).
    let mut to_describe: Vec<(String, CompletionRequest)> = Vec::new();
    for component in &asp_config.component {
        if !force {
            let already_described: bool = db.conn
                .query_row(
                    "SELECT 1 FROM ai_metadata WHERE component = ?1",
                    [&component.name],
                    |_| Ok(true),
                )
                .unwrap_or(false);
            if already_described {
                println!("Skipping '{}' (already described; use --force to regenerate)", component.name);
                continue;
            }
        }
        let context = build_component_context(&db, &root_str, component)?;
        to_describe.push((component.name.clone(), CompletionRequest {
            messages: vec![
                Message {
                    role: Role::System,
                    content: "You are a software architecture assistant. Write a concise one-to-two sentence description of a software component based on its files and structure.".to_string(),
                },
                Message {
                    role: Role::User,
                    content: format!(
                        "Describe the architectural role of the '{}' component.\n\n{}",
                        component.name, context
                    ),
                },
            ],
            max_tokens: Some(256),
            temperature: Some(0.3),
        }));
    }

    if to_describe.is_empty() {
        return Ok(());
    }

    // Phase 2: fire all LLM requests concurrently.
    println!("Describing {} component(s)...", to_describe.len());
    let client = OpenAiCompatClient::new(
        config.llm.base_url.clone(),
        config.llm.model.clone(),
        config.llm.api_key.clone(),
    );
    let futs: Vec<_> = to_describe
        .into_iter()
        .map(|(name, req)| {
            let c = client.clone();
            async move { (name, c.complete(req).await) }
        })
        .collect();
    let results = futures::future::join_all(futs).await;

    // Phase 3: persist results.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    for (name, result) in results {
        match result {
            Ok(rationale) => {
                db.conn.execute(
                    "INSERT OR REPLACE INTO ai_metadata (component, rationale, generated_at, model) \
                     VALUES (?1, ?2, ?3, ?4)",
                    rusqlite::params![name, rationale.trim(), now, config.llm.model],
                )?;
                println!("  {}: {}", name, rationale.trim());
            }
            Err(e) => eprintln!("  {}: failed: {}", name, e),
        }
    }

    Ok(())
}

/// Build a prompt context string for a component: matching files, their symbols and raw imports.
fn build_component_context(
    db: &asp_db::Database,
    root_str: &str,
    component: &asp_core::Component,
) -> Result<String> {
    use globset::{Glob, GlobSetBuilder};

    let mut builder = GlobSetBuilder::new();
    for pattern in &component.paths {
        if let Ok(g) = Glob::new(pattern) {
            builder.add(g);
        }
    }
    let glob_set = builder.build()?;

    let mut stmt = db.conn.prepare("SELECT path FROM files")?;
    let matching: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .filter_map(|r| r.ok())
        .filter(|p| glob_set.is_match(strip_root(p, root_str)))
        .collect();

    let mut lines = vec![
        format!("Component: {}", component.name),
        format!("Path patterns: {}", component.paths.join(", ")),
    ];

    if matching.is_empty() {
        lines.push("No indexed files found (project may need re-indexing).".to_string());
        return Ok(lines.join("\n"));
    }

    // Prepare statements once, reuse for each file.
    let mut sym_stmt = db.conn.prepare(
        "SELECT name, kind FROM symbols WHERE file = ?1 ORDER BY kind, name LIMIT ?2"
    )?;
    let mut imp_stmt = db.conn.prepare(
        "SELECT to_raw FROM imports WHERE from_file = ?1 LIMIT ?2"
    )?;

    lines.push(format!("Files ({}):", matching.len()));
    for path in &matching {
        lines.push(format!("  {}", strip_root(path, root_str)));

        let syms: Vec<String> = sym_stmt
            .query_map(rusqlite::params![path, MAX_CONTEXT_SYMBOLS as i64], |row| {
                Ok(format!("{} ({})", row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default();
        if !syms.is_empty() {
            lines.push(format!("    symbols: {}", syms.join(", ")));
        }

        let imps: Vec<String> = imp_stmt
            .query_map(rusqlite::params![path, MAX_CONTEXT_IMPORTS as i64], |row| row.get(0))
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default();
        if !imps.is_empty() {
            lines.push(format!("    imports: {}", imps.join("; ")));
        }
    }

    Ok(lines.join("\n"))
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
