pub const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS files (
    path        TEXT    PRIMARY KEY,
    hash        TEXT    NOT NULL,
    component   TEXT,
    last_parsed INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS imports (
    from_file   TEXT NOT NULL,
    to_file     TEXT,
    to_raw      TEXT NOT NULL,
    PRIMARY KEY (from_file, to_raw)
);

CREATE TABLE IF NOT EXISTS symbols (
    file        TEXT    NOT NULL,
    name        TEXT    NOT NULL,
    kind        TEXT    NOT NULL,
    line        INTEGER,
    exported    INTEGER DEFAULT 0,
    PRIMARY KEY (file, name)
);

CREATE TABLE IF NOT EXISTS components (
    name        TEXT PRIMARY KEY,
    description TEXT,
    paths_json  TEXT NOT NULL,
    owner       TEXT
);

CREATE TABLE IF NOT EXISTS rules (
    name        TEXT PRIMARY KEY,
    type        TEXT NOT NULL,
    config_json TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS ai_metadata (
    component       TEXT    PRIMARY KEY,
    rationale       TEXT,
    generated_at    INTEGER,
    model           TEXT
);

CREATE TABLE IF NOT EXISTS meta (
    key     TEXT PRIMARY KEY,
    value   TEXT
);
"#;
