use serde::{Deserialize, Serialize};
use std::fs;
use anyhow::Result;

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub atlassian_url: String,
    pub user_email: String,
    pub user_api_token: String,
    pub project: String,
}

impl Config {
    pub fn from_config_file() -> Result<Config> {
        let path = dirs::home_dir()
            .unwrap()
            .join(".config/jiratrack/config.toml");
        assert!(
            fs::exists(&path).unwrap(),
            "Config file not found. Ensure your config file is in ~/.config/jiratrack/config.toml"
        );
        let config = fs::read_to_string(&path)?;
        let config = toml::from_str::<Config>(&config)?;
        Ok(config)
    }
}
