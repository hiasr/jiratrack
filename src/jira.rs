use anyhow::Result;
use base64::{engine::general_purpose, Engine as _};
use jiff::{Unit, Zoned};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::HashMap, fs};

use ureq::{json, Error, Response};

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    atlassian_url: String,
    user_email: String,
    user_api_token: String,
}

impl Config {
    fn from_config_file() -> Result<Config> {
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

#[derive(Debug)]
pub struct Jira {
    atlassian_url: String,
    user_email: String,
    user_api_token: String,
}

#[derive(Debug, Clone)]
pub struct Issue {
    pub id: String,
    pub key: String,
    pub summary: String,
    pub time_spent: String,
    pub assignee: String,
}

fn create_basic_auth_header(user: &str, password: &str) -> String {
    let user_pass = String::from(user) + ":" + password;
    String::from("Basic ") + &general_purpose::STANDARD.encode(user_pass.as_bytes())
}

impl Jira {
    pub fn new() -> Self {
        let config = Config::from_config_file().unwrap();
        Jira {
            atlassian_url: config.atlassian_url,
            user_email: config.user_email,
            user_api_token: config.user_api_token,
        }
    }

    fn get_request(
        &self,
        endpoint: &str,
        params: Option<HashMap<String, String>>,
    ) -> Result<Response> {
        let url = format!("{}{endpoint}", &self.atlassian_url);

        let auth_header = create_basic_auth_header(&self.user_email, &self.user_api_token);
        let agent = ureq::AgentBuilder::new()
            .redirect_auth_headers(ureq::RedirectAuthHeaders::SameHost)
            .build();
        let mut request = agent
            .get(&url)
            .set("Accept", "application/json")
            .set("Authorization", &auth_header);

        if params.is_some() {
            for (key, value) in params.unwrap().into_iter() {
                request = request.query(&key, &value)
            }
        }

        let response = request.call()?;
        Ok(response)
    }

    fn post_request(
        &self,
        endpoint: &str,
        params: Option<HashMap<String, String>>,
        data: Option<Value>,
    ) -> Result<Response> {
        let url = format!("{}{endpoint}", &self.atlassian_url);

        let auth_header = create_basic_auth_header("ruben.hias@techwolf.ai", &self.user_api_token);
        let agent = ureq::AgentBuilder::new()
            .redirect_auth_headers(ureq::RedirectAuthHeaders::SameHost)
            .build();
        let mut request = agent
            .post(&url)
            .set("Accept", "application/json")
            .set("Authorization", &auth_header);

        if let Some(params) = params {
            for (key, value) in params.into_iter() {
                request = request.query(&key, &value)
            }
        }

        let response = match &data {
            Some(data) => request.send_json(data),
            None => request.call(),
        };

        let result = match response {
            Ok(result) => result,
            Err(Error::Status(_code, response)) => {
                panic!("{} {:?}", response.into_string().unwrap(), data)
            }
            _ => panic!("Request failed"),
        };

        Ok(result)
    }

    pub fn get_issue(&self, key: &str) -> Result<Issue> {
        let body = self
            .get_request(&format!("/rest/api/3/issue/{key}"), None)?
            .into_json()?;
        Ok(self.parse_issue(&body))
    }

    pub fn log_time(&self, issue_key: &str, started_on: &Zoned, ended_on: &Zoned) -> Result<()> {
        let time_spent_s = (ended_on - started_on).total(Unit::Second)?.floor() as u32;
        if time_spent_s < 60 {
            return Ok(());
        }
        let data = json!({
            "started": started_on.strftime("%Y-%m-%dT%H:%M:%S.%3f%z").to_string(),
            "timeSpentSeconds": time_spent_s,
        });
        let endpoint = format!("/rest/api/3/issue/{issue_key}/worklog");
        let result = self.post_request(&endpoint, None, Some(data));
        match result {
            Ok(_) => Ok(()),
            Err(err) => Err(err),
        }
    }

    pub fn assign_to_current_user(&self, issue_key: &str) -> Result<()> {
        let account_id = "-1";
        let data = json!({"accountId": account_id});
        let endpoint = format!("/rest/api/3/issue/{issue_key}/assignee");
        self.post_request(&endpoint, None, Some(data))?;
        Ok(())
    }

    fn get_issues_jql(&self, jql: &str) -> Result<Vec<Issue>> {
        let mut params = HashMap::new();
        params.insert("jql".to_string(), jql.to_string());
        params.insert(
            "fields".to_string(),
            "id,summary,key,timetracking,assignee".to_string(),
        );
        let data: serde_json::Value = self
            .get_request("/rest/api/3/search/jql", Some(params))?
            .into_json()?;

        let issues = data["issues"]
            .as_array()
            .unwrap()
            .iter()
            .map(|issue| self.parse_issue(issue))
            .collect();

        Ok(issues)
    }

    fn parse_issue(&self, issue: &serde_json::Value) -> Issue {
        Issue {
            id: issue["id"].as_str().unwrap().to_string(),
            key: issue["key"].as_str().unwrap().to_string(),
            summary: issue["fields"]["summary"].as_str().unwrap().to_string(),
            time_spent: issue["fields"]["timetracking"]["timeSpent"]
                .as_str()
                .unwrap_or("0h")
                .to_owned(),
            assignee: issue["fields"]["assignee"]["displayName"]
                .as_str()
                .unwrap_or("")
                .to_owned(),
        }
    }

    pub fn get_current_sprint_issues(&self) -> Result<Vec<Issue>> {
        let jql = "sprint in openSprints() AND project = \"IMG\" AND status != done AND status != archived";
        let issues = self.get_issues_jql(jql)?;
        Ok(issues)
    }
}

impl Default for Jira {
    fn default() -> Self {
        Jira::new()
    }
}

#[cfg(test)]
mod test {
    use jiff::ToSpan;

    use super::*;
    #[test]
    fn test_get_issue() {
        let api = Jira::new();
        if let Ok(issue) = api.get_issue("IMG-234") {
            println!("{:?}", issue)
        }
    }

    #[test]
    fn test_search_issues() {
        let api = Jira::new();
        if let Ok(issue) = api.get_current_sprint_issues() {
            println!("{:?}", issue)
        }
    }

    #[test]
    fn log_time() {
        let api = Jira::new();
        let started_on = &Zoned::now() - 10.minutes();
        let ended_on = Zoned::now();
        api.log_time("IMG-237", &started_on, &ended_on).unwrap()
    }

    #[test]
    fn test_assign() {
        let api = Jira::new();
        api.assign_to_current_user("IMG-266").unwrap()
    }

    #[test]
    fn test_ureq() {
        let url = "http://techwolf.atlassian.net/rest/api/2/issue/IMG-234";
        let body = ureq::get(url).call().unwrap().into_string().unwrap();
        println!("{}", body)
    }
}
