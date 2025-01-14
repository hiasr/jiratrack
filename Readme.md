# Jiratrack

A tui to easily record time worked on issues in Jira.

## Installation
1. Install Rust (e.g via [Rustup](https://rustup.rs/))
2. `cargo install jiratrack`

## Configuration
Jira track looks for a config file in `~/.config/jiratrack/config.toml`. 
Below you can find an example configuration file, all the options are required.

```toml
atlassian_url = "https://company.atlassian.net"
user_email = "john.doe@company.com"
user_api_token = "123456789abc"
```

You can find your API token [here](https://id.atlassian.com/manage-profile/security/api-tokens).

