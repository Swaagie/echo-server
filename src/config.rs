use clap::Parser;
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    H2c,
    H2,
    H3,
    Auto,
}

impl std::str::FromStr for Protocol {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "h2c" => Ok(Protocol::H2c),
            "h2" => Ok(Protocol::H2),
            "h3" => Ok(Protocol::H3),
            "auto" => Ok(Protocol::Auto),
            _ => Err(format!(
                "Unknown protocol: {}. Valid options: h2c, h2, h3, auto",
                s
            )),
        }
    }
}

impl<'de> Deserialize<'de> for Protocol {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Deserialize, Clone, PartialEq, Default)]
pub struct FileConfig {
    #[serde(default)]
    pub tls: Option<TlsConfig>,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub protocol: Option<Protocol>,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct TlsConfig {
    pub server_cert: String,
    pub server_key: String,
    #[serde(default)]
    pub ca_cert: Option<String>,
    #[serde(default = "default_require_client_certs")]
    pub require_client_certs: bool,
}

fn default_require_client_certs() -> bool {
    false
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub port: u16,
    pub tls: Option<TlsConfig>,
    pub protocol: Protocol,
}

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
    let config: FileConfig = toml::from_str(&contents)
        .map_err(|e| format!("Failed to parse TOML config file '{}': {}", path, e))?;
    Ok(Some(config))
}

fn resolve_path(config_file_path: &str, path: &str) -> PathBuf {
    let config_path = Path::new(config_file_path);
    let path = Path::new(path);

    if path.is_absolute() {
        path.to_path_buf()
    } else {
        let config_dir = config_path.parent().unwrap_or_else(|| Path::new("."));
        config_dir.join(path)
    }
}

fn reject_empty_paths(tls: &TlsConfig) -> Result<(), Box<dyn std::error::Error>> {
    let candidates = [
        ("server_cert", Some(&tls.server_cert)),
        ("server_key", Some(&tls.server_key)),
        ("ca_cert", tls.ca_cert.as_ref()),
    ];
    for (field, value) in candidates {
        if let Some(value) = value {
            if value.trim().is_empty() {
                return Err(
                    format!("TLS setting '{field}' is empty; set a path or remove it").into(),
                );
            }
        }
    }
    Ok(())
}

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
                .to_string(),
        );
    }
}

fn validate_tls_config(tls: &TlsConfig) -> Result<(), Box<dyn std::error::Error>> {
    if !Path::new(&tls.server_cert).exists() {
        return Err(format!("Server certificate file not found: {}", tls.server_cert).into());
    }
    if std::fs::File::open(&tls.server_cert).is_err() {
        return Err(format!("Cannot read server certificate file: {}", tls.server_cert).into());
    }

    if !Path::new(&tls.server_key).exists() {
        return Err(format!("Server key file not found: {}", tls.server_key).into());
    }
    if std::fs::File::open(&tls.server_key).is_err() {
        return Err(format!("Cannot read server key file: {}", tls.server_key).into());
    }

    if tls.require_client_certs {
        let ca_cert = tls
            .ca_cert
            .as_ref()
            .ok_or("CA certificate is required when require_client_certs is true")?;

        if !Path::new(ca_cert).exists() {
            return Err(format!("CA certificate file not found: {}", ca_cert).into());
        }
        if std::fs::File::open(ca_cert).is_err() {
            return Err(format!("Cannot read CA certificate file: {}", ca_cert).into());
        }
    }

    Ok(())
}

pub fn merge_config(cli: Cli) -> Result<AppConfig, Box<dyn std::error::Error>> {
    let file_config = load_config_file(&cli.config)?;

    let mut tls = file_config.as_ref().and_then(|c| c.tls.as_ref()).cloned();

    if let Some(ref mut tls_config) = tls {
        reject_empty_paths(tls_config)?;
        resolve_tls_paths(&cli.config, tls_config);
    }

    if let Some(ref tls_config) = tls {
        validate_tls_config(tls_config)?;
    }

    let protocol = if let Some(proto_str) = cli.protocol {
        proto_str.parse()?
    } else {
        file_config
            .as_ref()
            .and_then(|c| c.protocol)
            .unwrap_or(Protocol::Auto)
    };

    Ok(AppConfig {
        port: cli
            .port
            .or(file_config.as_ref().and_then(|c| c.port))
            .unwrap_or(8080),
        tls,
        protocol,
    })
}
