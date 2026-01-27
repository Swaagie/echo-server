use std::path::{Path, PathBuf};
use serde::Deserialize;
use clap::Parser;

/// Protocol selection for the server
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    /// HTTP/2 cleartext (h2c) - no TLS required
    H2c,
    /// HTTP/2 over TLS (h2) - requires TLS
    H2,
    /// HTTP/3 over QUIC (h3) - requires TLS
    H3,
    /// Auto-select based on TLS configuration
    /// - With TLS: Start both H2 (TCP) and H3 (UDP)
    /// - Without TLS: Start H2c (TCP) only
    Auto,
}

impl Protocol {
    pub fn from_str(s: &str) -> Result<Self, String> {
        match s.to_lowercase().as_str() {
            "h2c" => Ok(Protocol::H2c),
            "h2" => Ok(Protocol::H2),
            "h3" => Ok(Protocol::H3),
            "auto" => Ok(Protocol::Auto),
            _ => Err(format!("Unknown protocol: {}. Valid options: h2c, h2, h3, auto", s)),
        }
    }
}

impl<'de> Deserialize<'de> for Protocol {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Protocol::from_str(&s).map_err(serde::de::Error::custom)
    }
}

/// File-based configuration loaded from config file
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct FileConfig {
    /// Server certificate configuration
    #[serde(default)]
    pub tls: Option<TlsConfig>,
    /// Port to listen on
    #[serde(default)]
    pub port: Option<u16>,
    /// Protocol selection: "h2c", "h2", "h3", or "auto" (default)
    #[serde(default)]
    pub protocol: Option<Protocol>,
}

impl Default for FileConfig {
    fn default() -> Self {
        FileConfig {
            tls: None,
            port: None,
            protocol: Some(Protocol::Auto),
        }
    }
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct TlsConfig {
    /// Server certificate file path
    pub server_cert: String,
    /// Server private key file path
    pub server_key: String,
    /// CA certificate file path for client verification (required if require_client_certs is true)
    #[serde(default)]
    pub ca_cert: Option<String>,
    /// Require and validate client certificates (mTLS). Default: false
    #[serde(default = "default_require_client_certs")]
    pub require_client_certs: bool,
}

fn default_require_client_certs() -> bool {
    false
}

/// Merged configuration combining CLI args and config file
#[derive(Debug, Clone)]
pub struct AppConfig {
    pub port: u16,
    pub tls: Option<TlsConfig>,
    pub protocol: Protocol,
}

/// CLI arguments
#[derive(Debug, Parser)]
#[command(name = "echo-server", about = "HTTP echo server")]
pub struct Cli {
    /// Port to listen on
    #[arg(short, long)]
    pub port: Option<u16>,

    /// Path to config file
    #[arg(long, default_value = "config.toml")]
    pub config: String,

    /// Protocol selection: "h2c", "h2", "h3", or "auto" (default)
    #[arg(long)]
    pub protocol: Option<String>,
}

pub fn load_config_file(path: &str) -> Result<Option<FileConfig>, Box<dyn std::error::Error>> {
    if !Path::new(path).exists() {
        return Ok(None);
    }

    let contents = std::fs::read_to_string(path)?;
    let config: FileConfig = toml::from_str(&contents).map_err(|e| {
        format!("Failed to parse TOML config file '{}': {}", path, e)
    })?;
    Ok(Some(config))
}

/// Resolve a path relative to the config file directory.
/// If the path is already absolute, it is returned as-is.
/// If the path is relative, it is resolved relative to the config file's directory.
fn resolve_path(config_file_path: &str, path: &str) -> PathBuf {
    let config_path = Path::new(config_file_path);
    let path = Path::new(path);

    if path.is_absolute() {
        path.to_path_buf()
    } else {
        // Get the directory containing the config file
        let config_dir = config_path.parent()
            .unwrap_or_else(|| Path::new("."));
        config_dir.join(path)
    }
}

/// Resolve all certificate paths in TLS config relative to the config file directory
fn resolve_tls_paths(config_file_path: &str, tls: &mut TlsConfig) {
    tls.server_cert = resolve_path(config_file_path, &tls.server_cert)
        .to_string_lossy()
        .to_string();
    tls.server_key = resolve_path(config_file_path, &tls.server_key)
        .to_string_lossy()
        .to_string();
    if let Some(ref ca_cert) = tls.ca_cert {
        tls.ca_cert = Some(
            resolve_path(config_file_path, ca_cert)
                .to_string_lossy()
                .to_string()
        );
    }
}

/// Validate that TLS certificate files exist and can be read
fn validate_tls_config(tls: &TlsConfig) -> Result<(), Box<dyn std::error::Error>> {
    // Check server certificate
    if !Path::new(&tls.server_cert).exists() {
        return Err(format!("Server certificate file not found: {}", tls.server_cert).into());
    }
    if std::fs::read(&tls.server_cert).is_err() {
        return Err(format!("Cannot read server certificate file: {}", tls.server_cert).into());
    }

    // Check server key
    if !Path::new(&tls.server_key).exists() {
        return Err(format!("Server key file not found: {}", tls.server_key).into());
    }
    if std::fs::read(&tls.server_key).is_err() {
        return Err(format!("Cannot read server key file: {}", tls.server_key).into());
    }

    // Check CA certificate if client cert validation is required
    if tls.require_client_certs {
        let ca_cert = tls.ca_cert.as_ref().ok_or(
            "CA certificate is required when require_client_certs is true"
        )?;

        if !Path::new(ca_cert).exists() {
            return Err(format!("CA certificate file not found: {}", ca_cert).into());
        }
        if std::fs::read(ca_cert).is_err() {
            return Err(format!("Cannot read CA certificate file: {}", ca_cert).into());
        }
    }

    Ok(())
}

pub fn merge_config(cli: Cli) -> Result<AppConfig, Box<dyn std::error::Error>> {
    let file_config = load_config_file(&cli.config)?;

    // Get TLS config from config file only
    let mut tls = file_config.as_ref().and_then(|c| c.tls.as_ref()).cloned();

    // Resolve relative paths relative to the config file directory
    if let Some(ref mut tls_config) = tls {
        resolve_tls_paths(&cli.config, tls_config);
    }

    // Validate TLS config if present (after resolving paths)
    if let Some(ref tls_config) = tls {
        validate_tls_config(tls_config)?;
    }

    // Protocol selection: CLI takes precedence over config file, default to Auto
    let protocol = if let Some(proto_str) = cli.protocol {
        Protocol::from_str(&proto_str)?
    } else {
        file_config
            .as_ref()
            .and_then(|c| c.protocol)
            .unwrap_or(Protocol::Auto)
    };

    Ok(AppConfig {
        port: cli.port.or(file_config.as_ref().and_then(|c| c.port)).unwrap_or(8080),
        tls,
        protocol,
    })
}

