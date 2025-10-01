use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Layout},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Row, Table},
    Terminal,
};
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use sysinfo::{System, Process, ProcessesToUpdate};
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // Initialize terminal
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut sys = System::new_all();
    // Initial refresh to get CPU info
    sys.refresh_cpu_all();

    // Get CPU model (assume first CPU for model name, as it's typically the same for all cores)
    let cpu_model = sys.cpus().first().map_or("Unknown".to_string(), |cpu| cpu.brand().to_string());

    loop {
        // Refresh system information
        sys.refresh_cpu_all();
        sys.refresh_processes(ProcessesToUpdate::All);

        // Draw UI
        terminal.draw(|f| {
            let size = f.area();

            // Layout: Vertical split for system info and process table
            let chunks = Layout::default()
                .constraints([Constraint::Length(4), Constraint::Min(0)])
                .split(size);

            // System information (CPU model and usage)
            let system_text = vec![
                Line::from(Span::styled(
                    format!("CPU Model: {}", cpu_model),
                    Style::default().fg(Color::Green),
                )),
                Line::from(Span::styled(
                    format!("Total CPU Usage: {:.2}%", sys.global_cpu_usage()),
                    Style::default().fg(Color::Yellow),
                )),
            ];
            let system_block = Block::default()
                .title("System")
                .borders(Borders::ALL)
                .style(Style::default().fg(Color::White));
            f.render_widget(
                ratatui::widgets::Paragraph::new(system_text).block(system_block),
                chunks[0],
            );

            // Top 5 processes by CPU usage
            let mut procs: Vec<&Process> = sys.processes().values().collect();
            procs.sort_by(|a, b| b.cpu_usage().partial_cmp(&a.cpu_usage()).unwrap());

            let rows: Vec<Row> = procs
                .iter()
                .take(5)
                .map(|p| {
                    Row::new(vec![
                        p.name().to_string_lossy().to_string(),
                        p.pid().to_string(),
                        format!("{:.2}%", p.cpu_usage()),
                    ])
                })
                .collect();

            let table = Table::new(
                rows,
                [
                    Constraint::Percentage(50), // Name
                    Constraint::Percentage(20), // PID
                    Constraint::Percentage(30), // CPU Usage
                ],
            )
            .header(
                Row::new(vec!["Name", "PID", "CPU Usage"])
                    .style(Style::default().fg(Color::Cyan))
                    .bottom_margin(1),
            )
            .block(
                Block::default()
                    .title("Top 5 Processes")
                    .borders(Borders::ALL),
            )
            .style(Style::default().fg(Color::White));

            f.render_widget(table, chunks[1]);
        })?;

        // Handle input (quit on 'q')
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.code == KeyCode::Char('q') {
                    break;
                }
            }
        }

        // Sleep for 1 second
        sleep(Duration::from_millis(1000)).await;
    }

    // Clean up terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}