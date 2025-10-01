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
#[cfg(feature = "gpu")]
use nvml_wrapper::Nvml;
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
    // Initial refresh to get system info
    sys.refresh_all();

    // Get CPU model (assume first CPU for model name, as it's typically the same for all cores)
    let cpu_model = sys.cpus().first().map_or("Unknown".to_string(), |cpu| cpu.brand().to_string());

    // Initialize NVML for GPU monitoring (if enabled)
    #[cfg(feature = "gpu")]
    let nvml = Nvml::init().ok(); // Handle initialization failure gracefully

    loop {
        // Refresh system information
        sys.refresh_all(); // Refreshes CPU, memory, and processes
        sys.refresh_cpu_all(); // Specific refresh for CPU usage
        sys.refresh_processes(ProcessesToUpdate::All);

        // Draw UI
        terminal.draw(|f| {
            let size = f.area();

            // Layout: Vertical split for system info and process table
            let chunks = Layout::default()
                .constraints([Constraint::Length(6), Constraint::Min(0)]) // Increased height for more info
                .split(size);

            // System information (CPU model, usage, RAM, and GPU)
            let mut system_text = vec![
                Line::from(Span::styled(
                    format!("CPU Model: {}", cpu_model),
                    Style::default().fg(Color::Green),
                )),
                Line::from(Span::styled(
                    format!("Total CPU Usage: {:.2}%", sys.global_cpu_usage()),
                    Style::default().fg(Color::Yellow),
                )),
                Line::from(Span::styled(
                    format!(
                        "RAM: {}/{} MB ({:.2}%)",
                        sys.used_memory() / 1024, // Convert KB to MB
                        sys.total_memory() / 1024,
                        (sys.used_memory() as f64 / sys.total_memory() as f64) * 100.0
                    ),
                    Style::default().fg(Color::Blue),
                )),
            ];

            // Add GPU information if NVML is enabled and initialized
            #[cfg(feature = "gpu")]
            if let Some(nvml) = &nvml {
                if let Ok(device) = nvml.device_by_index(0) {
                    if let Ok(utilization) = device.utilization_rates() {
                        system_text.push(Line::from(Span::styled(
                            format!("GPU Utilization: {}%", utilization.gpu),
                            Style::default().fg(Color::Magenta),
                        )));
                        if let Ok(memory) = device.memory_info() {
                            system_text.push(Line::from(Span::styled(
                                format!(
                                    "GPU Memory: {}/{} MB ({:.2}%)",
                                    memory.used / 1024 / 1024, // Convert bytes to MB
                                    memory.total / 1024 / 1024,
                                    (memory.used as f64 / memory.total as f64) * 100.0
                                ),
                                Style::default().fg(Color::Magenta),
                            )));
                        }
                    }
                } else {
                    system_text.push(Line::from(Span::styled(
                        "GPU: Not detected".to_string(),
                        Style::default().fg(Color::Red),
                    )));
                }
            }
            #[cfg(not(feature = "gpu"))]
            system_text.push(Line::from(Span::styled(
                "GPU: Monitoring disabled".to_string(),
                Style::default().fg(Color::Red),
            )));

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