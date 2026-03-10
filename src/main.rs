use std::{env, fs, path::PathBuf, time::Duration};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use figment::{
    Figment,
    providers::{Format, Serialized, Toml},
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::daemon::{start_daemon, status_daemon, stop_daemon};

mod daemon;
mod highlighter;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Start the highlighter daemon if it's not already running
    Start,

    /// Stop the highlighter daemon if it's not already stopped
    Stop,

    /// Restart the highlighter daemon or make sure it is started if it's not
    /// running
    Restart,

    /// Check whether the highlighter daemon is running
    Status,
}

#[derive(Serialize, Deserialize, Default)]
pub struct Config {
    pub highlighting: HighlightingConfig,
}

#[derive(Serialize, Deserialize)]
pub struct HighlightingConfig {
    /// For performance reasons, highlighting is disabled for very long lines.
    /// This option specifies the maximum length of a line (in bytes) up to
    /// which highlighting is applied.
    pub max_line_length: usize,

    /// The maximum time (in milliseconds) to spend on highlighting a command.
    /// If highlighting takes longer, it will be aborted and the command will be
    /// partially highlighted.
    ///
    /// Note that the timeout only applies to multi-line commands. Highlighting
    /// cannot be aborted in the middle of a line. If you often deal with long
    /// lines that take longer to highlight than the timeout, consider reducing
    /// [max_line_length](Self::max_line_length).
    #[serde(
        rename = "timeout_ms",
        serialize_with = "serialize_duration_ms",
        deserialize_with = "deserialize_duration_ms"
    )]
    pub timeout: Duration,
}

fn serialize_duration_ms<S: Serializer>(duration: &Duration, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_u64(duration.as_millis() as u64)
}

fn deserialize_duration_ms<'de, D: Deserializer<'de>>(d: D) -> Result<Duration, D::Error> {
    let ms = u64::deserialize(d)?;
    Ok(Duration::from_millis(ms))
}

impl Default for HighlightingConfig {
    fn default() -> Self {
        Self {
            max_line_length: 20000,
            timeout: Duration::from_millis(500),
        }
    }
}

fn main() -> Result<()> {
    let home = PathBuf::from(env::var("HOME").context("$HOME not set")?);
    let config_dir = home.join(".config/zsh-patina");
    let data_dir = home.join(".local/share/zsh-patina");

    // parse arguments
    let args = Args::parse();

    // load config file
    let config_file_path = config_dir.join("config.toml");
    let config = if fs::exists(&config_file_path)? {
        Figment::new()
            .merge(Serialized::defaults(Config::default()))
            .merge(Toml::file(&config_file_path))
            .extract()
            .with_context(|| format!("Unable to read config file {config_file_path:?}"))?
    } else {
        Config::default()
    };

    match args.command {
        Command::Start => start_daemon(&data_dir, &config),
        Command::Stop => stop_daemon(&data_dir),
        Command::Restart => {
            stop_daemon(&data_dir)?;
            start_daemon(&data_dir, &config)
        }
        Command::Status => status_daemon(&data_dir),
    }
}
