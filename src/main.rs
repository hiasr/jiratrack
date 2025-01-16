use std::{
    fs::{self, File},
    path::PathBuf,
};

use anyhow::Result;
use arboard::Clipboard;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use dirs::home_dir;
use fuzzy_matcher::clangd::fuzzy_match;
use jiff::{Unit, Zoned};
use jira::{Issue, Jira};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style, Stylize},
    symbols::border,
    text::{Line, Text},
    widgets::{Block, Cell, Paragraph, Row, Table, TableState},
    DefaultTerminal, Frame,
};
use serde::{Deserialize, Serialize};
pub mod jira;
pub mod config;

fn main() -> Result<()> {
    let mut terminal = ratatui::init();
    let app_result = App::new().run(&mut terminal);
    ratatui::restore();
    app_result
}
#[derive(Debug, Serialize, Deserialize)]
pub struct PersistedState {
    active_issue: Option<String>,
    activated_on: Option<Zoned>,
}

#[derive(Debug, Default)]
pub struct App {
    jira: Jira,
    issues: Vec<Issue>,
    search_input: String,
    filtered_issues: Vec<Issue>,

    active_issue: Option<String>,
    activated_on: Option<Zoned>,

    table_state: TableState,
    colors: AppColor,
    exit: bool,
}

impl App {
    pub fn new() -> Self {
        App {
            jira: Jira::new(),
            issues: vec![],
            filtered_issues: vec![],
            search_input: "".to_string(),
            active_issue: None,
            activated_on: None,

            table_state: TableState::default().with_selected(Some(0)),
            colors: AppColor::default(),
            exit: false,
        }
    }
    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        self.issues = self.jira.get_current_sprint_issues()?;
        self.load_state();

        while !self.exit {
            terminal.draw(|frame| self.draw(frame))?;
            self.handle_events()?;
        }
        Ok(())
    }

    fn draw(&mut self, frame: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(1),
                Constraint::Length(3),
                Constraint::Length(3),
            ])
            .split(frame.area());

        self.filtered_issues = self.search_issues();
        self.render_issue_list(frame, chunks[0]);
        self.render_current_issue(frame, chunks[1]);
        self.render_search(frame, chunks[2]);
    }

    fn handle_events(&mut self) -> Result<()> {
        match event::read()? {
            Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                self.handle_key_event(key_event)
            }
            _ => {}
        };
        Ok(())
    }

    fn handle_key_event(&mut self, key_event: KeyEvent) {
        if key_event.modifiers.contains(KeyModifiers::CONTROL) {
            match key_event.code {
                KeyCode::Char('s') => self.deactivate_issue(),
                KeyCode::Char('d') => self.clear_state(),
                KeyCode::Char('y') => self.copy_mr_title(),
                _ => ()
            }
            return
        } 
        match key_event.code {
            KeyCode::Esc => self.exit(),
            KeyCode::Down => self.table_state.select_next(),
            KeyCode::Up => self.table_state.select_previous(),
            KeyCode::Char(char) => self.add_char(char),
            KeyCode::Backspace => self.delete_char(),
            KeyCode::Enter => self.activate_issue(),
            _ => {}
        }
    }

    fn activate_issue(&mut self) {
        self.deactivate_issue();
        self.active_issue = if let Some(issue_index) = self.table_state.selected() {
            Some(self.filtered_issues.get(issue_index).unwrap().key.clone())
        } else {
            return;
        };
        self.activated_on = Some(Zoned::now());
        self.persist_state()
    }

    fn deactivate_issue(&mut self) {
        if let (Some(active_issue), Some(activated_on)) = (&self.active_issue, &self.activated_on) {
            self.jira
                .log_time(active_issue, activated_on, &Zoned::now())
                .unwrap();
        }
        self.clear_state();
    }

    fn clear_state(&mut self) {
        self.active_issue = None;
        self.activated_on = None;
        self.persist_state();
    }

    fn copy_mr_title(&self) {
        let issue = self.get_active_issue();

        if let Some(issue) = issue {
        let issue_string = format!("[{}] {}", issue.key, issue.summary);

        let mut clipboard = Clipboard::new().unwrap();
        clipboard.set_text(issue_string).unwrap();
        }
    }

    fn add_char(&mut self, new_char: char) {
        self.search_input.push(new_char)
    }

    fn delete_char(&mut self) {
        let mut chars = self.search_input.chars();
        chars.next_back();
        self.search_input = chars.as_str().to_string()
    }

    fn exit(&mut self) {
        self.exit = true;
    }

    fn render_issue_list(&mut self, frame: &mut Frame, area: Rect) {
        let title = Line::from(" Jiratrack ".bold());
        let instructions = Line::from(vec![
            " Activate Issue ".into(),
            "<Enter>  ".blue().bold(),
            " Submit Worklog ".into(),
            "<C-s>  ".blue().bold(),
            " Cancel Worklog ".into(),
            "<C-d>  ".blue().bold(),
            " Copy Active MR Title ".into(),
            "<C-y>  ".blue().bold(),
            " Quit ".into(),
            "<esc> ".blue().bold(),
        ]);

        let selected_style = Style::default().bg(self.colors.selected_bg_color);

        let block = Block::bordered()
            .title(title.centered())
            .title_bottom(instructions.centered())
            .border_set(border::THICK);

        let header = ["Key", "Time Spent", "Assignee", "Title"]
            .into_iter()
            .map(Cell::from)
            .collect::<Row>()
            .height(1);

        let rows: Vec<Row> = self
            .filtered_issues
            .iter()
            .map(|issue| {
                let cols = [
                    &issue.key,
                    &issue.time_spent,
                    &issue.assignee,
                    &issue.summary,
                ];
                cols.iter()
                    .map(|content| Cell::from(Text::from(content.to_string())))
                    .collect::<Row>()
                    .height(1)
            })
            .collect();

        let table = Table::new(
            rows,
            [
                Constraint::Length(10),
                Constraint::Length(12),
                Constraint::Length(20),
                Constraint::Min(20),
            ],
        )
        .header(header)
        .row_highlight_style(selected_style)
        .highlight_symbol(">>")
        .block(block);
        frame.render_stateful_widget(table, area, &mut self.table_state);
    }

    fn render_current_issue(&self, frame: &mut Frame, area: Rect) {
        let title = Line::from("  Current Issue  ".bold());
        let block = Block::bordered().title(title);
        let duration = match &self.activated_on {
            Some(zoned) => format!("{:#}", (&Zoned::now() - zoned).round(Unit::Second).unwrap()),
            None => "/".to_string(),
        };

        let text = match &self.get_active_issue() {
            Some(issue) => format!(" {} {} ({})", issue.key, issue.summary, duration),
            None => " No issue active".to_string(),
        };
        let p = Paragraph::new(text).block(block);

        frame.render_widget(p, area)
    }

    fn get_active_issue(&self) -> Option<Issue> {
        let active_issue = self.active_issue.as_ref()?;
        self.issues
            .iter()
            .find(|issue| &issue.key == active_issue)
            .cloned()
    }

    fn search_issues(&self) -> Vec<Issue> {
        let mut issues = self
            .issues
            .iter()
            .filter_map(|issue| Some((issue, fuzzy_match(&issue.summary, &self.search_input)?)))
            .collect::<Vec<(&Issue, i64)>>();
        issues.sort_by_key(|(_, score)| -*score);
        issues
            .into_iter()
            .map(|(issue, _)| issue)
            .cloned()
            .collect()
    }

    fn render_search(&self, frame: &mut Frame, area: Rect) {
        let title = Line::from(" Search Input ".bold());
        let block = Block::bordered().title(title);

        let p = Paragraph::new("> ".to_string() + &self.search_input).block(block);

        frame.render_widget(p, area)
    }

    fn get_state_path(&self) -> PathBuf {
        home_dir()
            .unwrap()
            .join(".local/share/jiratrack/state.json")
    }

    fn persist_state(&self) {
        let path = self.get_state_path();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let state = self.get_persisted_state();
        let file = fs::File::create(path).unwrap();
        serde_json::to_writer(file, &state).unwrap();
    }

    fn load_state(&mut self) {
        let path = self.get_state_path();
        if let Ok(file) = File::open(path) {
            let data: PersistedState = serde_json::from_reader(file).expect("Invalid state");
            self.active_issue = data.active_issue;
            self.activated_on = data.activated_on;
        }
    }

    fn get_persisted_state(&self) -> PersistedState {
        PersistedState {
            active_issue: self.active_issue.clone(),
            activated_on: self.activated_on.clone(),
        }
    }
}

#[derive(Debug)]
struct AppColor {
    selected_bg_color: Color,
}

impl Default for AppColor {
    fn default() -> Self {
        AppColor {
            selected_bg_color: Color::DarkGray,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_filter_issues() {
        let mut app = App::new();
        app.issues = app.jira.get_current_sprint_issues().unwrap();
        app.search_issues();
    }

    #[test]
    fn test_persist_state() {
        let app = App::new();
        app.persist_state();
        assert!(fs::exists("/Users/rubenh/.local/share/jiratrack/state.json").unwrap())
    }
}
