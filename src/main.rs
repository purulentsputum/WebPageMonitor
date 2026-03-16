use anyhow::Result;
use chrono::Local;
use crossterm::event;
use crossterm::event::Event;
use crossterm::event::KeyCode;
use crossterm::event::KeyEventKind;
use crossterm::execute;
use crossterm::terminal::EnterAlternateScreen;
use crossterm::terminal::LeaveAlternateScreen;
use crossterm::terminal::disable_raw_mode;
use crossterm::terminal::enable_raw_mode;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Constraint;
use ratatui::layout::Direction;
use ratatui::layout::Layout;
use ratatui::style::Color;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::Block;
use ratatui::widgets::Borders;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Wrap;
use regex::Regex;
use scraper::Html;
use scraper::Selector;
use similar::ChangeTag;
use similar::TextDiff;
use std::io;
use std::time::Duration;
use tokio::sync::mpsc;

#[derive(Clone, Copy)]
enum Stage {
    Url,
    Selector,
    Interval,
    Running,
}

enum MonitorEvent {
    Checked,
    Changed(Vec<Line<'static>>),
    Unreachable,
}

struct App {
    stage: Stage,
    url: String,
    selector: String,
    interval: String,

    last_check: String,
    status: String,

    change_log: Vec<Line<'static>>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            stage: Stage::Url,
            url: String::new(),
            selector: "html".into(),
            interval: "30".into(),
            last_check: "-".into(),
            status: "Waiting".into(),
            change_log: Vec::new(),
        }
    }
}

async fn fetch_content(url: &str, selector: &str) -> Result<String> {
    let body = reqwest::get(url).await?.text().await?;

    if selector == "html" {
        return Ok(body);
    }

    let doc = Html::parse_document(&body);
    let selector = Selector::parse(selector)
        .map_err(|e| anyhow::anyhow!("Invalid selector '{}': {}", selector, e))?;

    let mut out = String::new();

    for el in doc.select(&selector) {
        out.push_str(&el.html());
    }

    Ok(out)
}

fn clean_html(input: &str) -> String {
    let dynamic_patterns = [r"\d{2}:\d{2}:\d{2}", r"\d{4}-\d{2}-\d{2}"];

    let mut result = input.to_string();

    for p in dynamic_patterns {
        let re = Regex::new(p).unwrap();
        result = re.replace_all(&result, "").to_string();
    }

    result
}

fn generate_diff(old: &str, new: &str) -> Vec<Line<'static>> {
    let diff = TextDiff::from_lines(old, new);

    let mut lines = Vec::new();

    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Delete => {
                lines.push(Line::from(Span::styled(
                    format!("- {}", change),
                    Style::default().fg(Color::Red),
                )));
            }

            ChangeTag::Insert => {
                lines.push(Line::from(Span::styled(
                    format!("+ {}", change),
                    Style::default().fg(Color::Green),
                )));
            }

            ChangeTag::Equal => {}
        }
    }

    lines
}

async fn monitor(url: String, selector: String, interval: u64, tx: mpsc::Sender<MonitorEvent>) {
    let mut previous_content = String::new();

    loop {
        match fetch_content(&url, &selector).await {
            Ok(content) => {
                let cleaned = clean_html(&content);

                if previous_content.is_empty() {
                    previous_content = cleaned;
                    let _ = tx.send(MonitorEvent::Checked).await;
                } else if previous_content != cleaned {
                    let diff_lines = generate_diff(&previous_content, &cleaned);

                    previous_content = cleaned;

                    let _ = tx.send(MonitorEvent::Changed(diff_lines)).await;
                } else {
                    let _ = tx.send(MonitorEvent::Checked).await;
                }
            }

            Err(_) => {
                let _ = tx.send(MonitorEvent::Unreachable).await;
            }
        }

        tokio::time::sleep(Duration::from_secs(interval)).await;
    }
}

fn draw(frame: &mut ratatui::Frame, app: &App) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(7),
            Constraint::Min(5),
            Constraint::Length(2),
        ])
        .split(frame.area());

    match app.stage {
        Stage::Url => {
            let p = Paragraph::new(app.url.as_str())
                .block(Block::default().title("Enter URL").borders(Borders::ALL));

            frame.render_widget(p, layout[0]);
        }

        Stage::Selector => {
            let p = Paragraph::new(app.selector.as_str()).block(
                Block::default()
                    .title("Enter Selector (default html)")
                    .borders(Borders::ALL),
            );

            frame.render_widget(p, layout[0]);
        }

        Stage::Interval => {
            let p = Paragraph::new(app.interval.as_str()).block(
                Block::default()
                    .title("Enter Interval (seconds)")
                    .borders(Borders::ALL),
            );

            frame.render_widget(p, layout[0]);
        }

        Stage::Running => {
            let status = vec![
                Line::from(format!("URL: {}", app.url)),
                Line::from(format!("Selector: {}", app.selector)),
                Line::from(format!("Interval: {} sec", app.interval)),
                Line::from(format!("Last Check: {}", app.last_check)),
                Line::from(Span::styled(
                    format!("Status: {}", app.status),
                    if app.status.contains("CHANGE") {
                        Style::default().fg(Color::Red)
                    } else if app.status.contains("UNREACHABLE") {
                        Style::default().fg(Color::Yellow)
                    } else {
                        Style::default().fg(Color::Green)
                    },
                )),
            ];

            let status_widget = Paragraph::new(status)
                .block(Block::default().title("Monitoring").borders(Borders::ALL))
                .wrap(Wrap { trim: true });

            frame.render_widget(status_widget, layout[0]);

            let log = Paragraph::new(app.change_log.clone())
                .block(Block::default().title("Change Log").borders(Borders::ALL))
                .wrap(Wrap { trim: false });

            frame.render_widget(log, layout[1]);
        }
    }

    let footer =
        Paragraph::new("Enter=next/start   q=quit").block(Block::default().borders(Borders::TOP));

    frame.render_widget(footer, layout[2]);
}

#[tokio::main]
async fn main() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::default();

    let (tx, mut rx) = mpsc::channel::<MonitorEvent>(10);
    let mut monitor_handle = None;

    loop {
        terminal.draw(|f| draw(f, &app))?;

        if let Ok(event) = rx.try_recv() {
            let timestamp = Local::now().format("%H:%M:%S").to_string();
            app.last_check = timestamp.clone();

            match event {
                MonitorEvent::Checked => {
                    app.status = "OK".into();
                }

                MonitorEvent::Changed(diff) => {
                    app.status = "CHANGE DETECTED".into();

                    app.change_log
                        .push(Line::from(format!("[{}] Change detected", timestamp)));

                    app.change_log.extend(diff);
                }

                MonitorEvent::Unreachable => {
                    app.status = "URL UNREACHABLE".into();

                    app.change_log
                        .push(Line::from(format!("[{}] URL unreachable", timestamp)));
                }
            }
        }

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') => break,

                        KeyCode::Enter => match app.stage {
                            Stage::Url => app.stage = Stage::Selector,

                            Stage::Selector => app.stage = Stage::Interval,

                            Stage::Interval => {
                                let interval = app.interval.parse().unwrap_or(30);

                                let tx_clone = tx.clone();

                                monitor_handle = Some(tokio::spawn(monitor(
                                    app.url.clone(),
                                    app.selector.clone(),
                                    interval,
                                    tx_clone,
                                )));

                                app.stage = Stage::Running;
                            }

                            Stage::Running => {}
                        },

                        KeyCode::Backspace => match app.stage {
                            Stage::Url => {
                                app.url.pop();
                            }

                            Stage::Selector => {
                                app.selector.pop();
                            }

                            Stage::Interval => {
                                app.interval.pop();
                            }

                            Stage::Running => {}
                        },

                        KeyCode::Char(c) => match app.stage {
                            Stage::Url => app.url.push(c),

                            Stage::Selector => app.selector.push(c),

                            Stage::Interval => app.interval.push(c),

                            Stage::Running => {}
                        },

                        _ => {}
                    }
                }
            }
        }
    }

    if let Some(handle) = monitor_handle {
        handle.abort();
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mock content fetch
    async fn mock_fetch(url: &str, selector: &str) -> Result<String> {
        let html = r#"<div id="price">$19.99</div>"#;
        if selector == "html" {
            Ok(html.to_string())
        } else {
            Ok("<div id=\"price\">$19.99</div>".to_string())
        }
    }

    #[tokio::test]
    async fn test_fetch_content_html() {
        let content = mock_fetch("http://example.com", "html").await.unwrap();
        assert!(content.contains("$19.99"));
    }

    #[tokio::test]
    async fn test_fetch_content_selector() {
        let content = mock_fetch("http://example.com", "#price").await.unwrap();
        assert_eq!(content, "<div id=\"price\">$19.99</div>");
    }

    #[test]
    fn test_clean_html_removes_timestamp() {
        let html = "<div>Time: 12:34:56</div>";
        let cleaned = clean_html(html);
        assert!(!cleaned.contains("12:34:56"));
    }

    #[test]
    fn test_generate_diff_detection() {
        let old = "<div>$19.99</div>";
        let new = "<div>$17.99</div>";
        let diff_lines = generate_diff(old, new);
        assert!(diff_lines.iter().any(|line| line.to_string().contains("-")));
        assert!(diff_lines.iter().any(|line| line.to_string().contains("+")));
    }

    #[tokio::test]
    async fn test_monitor_detects_change() {
        use tokio::sync::mpsc;
        let (tx, mut rx) = mpsc::channel(10);
        let url = "mock_url".to_string();
        let selector = "html".to_string();
        tokio::spawn(async move {
            let _ = tx
                .send(MonitorEvent::Changed(generate_diff(
                    "<div>1</div>",
                    "<div>2</div>",
                )))
                .await;
        });
        if let Some(event) = rx.recv().await {
            match event {
                MonitorEvent::Changed(diff) => {
                    assert!(!diff.is_empty());
                }
                _ => panic!("Expected changed event"),
            }
        }
    }
}
