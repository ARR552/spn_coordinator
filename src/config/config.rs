use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub artifact_base_url: String,       // e.g. "http://localhost:8082/artifacts"
    pub log_level: String,          // e.g. "info", "warn", "error", "debug", "trace"
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub grpc_addr: String,                 // e.g. "0.0.0.0"
    pub grpc_port: u16,                  // e.g. 50051
    pub grpc_tls: bool,               // e.g. false
    pub http_addr: String,          // e.g. "0.0.0.0"
    pub http_port: u16,                  // e.g. 8082
    pub db_path: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            artifact_base_url: "http://localhost:8082/artifacts".into(),
            log_level: "".into(),
        }
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            grpc_addr: "localhost:50051".into(),
            grpc_port: 50051,
            grpc_tls: false,
            http_addr: "0.0.0.0".into(),
            http_port: 8082,
            db_path: "./db/".into(),
        }
    }
}

pub fn load_config(explicit_path: Option<PathBuf>) -> anyhow::Result<Config> {
    // Start with defaults
    let mut builder = config::Config::builder()
        .set_default("server.grpc_addr", Config::default().server.grpc_addr)?
        .set_default("server.grpc_port", Config::default().server.grpc_port)?
        .set_default("server.grpc_tls", Config::default().server.grpc_tls)?
        .set_default("server.http_addr", Config::default().server.http_addr)?
        .set_default("server.http_port", Config::default().server.http_port)?
        .set_default("artifact_base_url", Config::default().artifact_base_url)?
        .set_default("log_level", Config::default().log_level.clone())?;

    // 1) Explicit --config path (if provided)
    if let Some(path) = explicit_path {
        builder = builder.add_source(config::File::from(path).required(true));
    } else {
        // 2) Conventional locations (first existing wins)
        //   ./config/{default,local}.{toml,yaml,json}
        //   ${XDG_CONFIG_HOME}/spn_coordinator/config.toml (or ~/.config/…)
        let candidates = [
            "config/local.toml",
            "config/local.yaml",
            "config/local.json",
            "config/default.toml",
            "config/default.yaml",
            "config/default.json",
        ];
        for c in candidates {
            let p = std::path::Path::new(c);
            if p.exists() {
                builder = builder.add_source(config::File::from(p).required(false));
                break;
            }
        }

        // XDG location
        if let Some(dir) = directories::ProjectDirs::from("tech", "", "spn_coordinator")
            .map(|d| d.config_dir().to_path_buf())
        {
            let xdg = dir.join("config.toml");
            if xdg.exists() {
                builder = builder.add_source(config::File::from(xdg).required(false));
            }
        }
    }

    // 3) Environment variables:
    // Prefix SPN_COORDINATOR, nested with _  e.g.:
    //   SPN_COORDINATOR_LOG_LEVEL=debug
    builder = builder.add_source(
        config::Environment::with_prefix("SPN_COORDINATOR")
            .separator("_")
            .try_parsing(true)
            .list_separator(","),
    );

    // Build and deserialize
    let cfg = builder.build()?;
    let s: Config = cfg.try_deserialize()?;

    // Optional: if LOG_LEVEL env/config is set, export it for `tracing_subscriber` EnvFilter
    if !s.log_level.is_empty() {
         std::env::set_var("RUST_LOG", &s.log_level);
    }

    Ok(s)
}