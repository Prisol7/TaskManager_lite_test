use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style, Modifier},
    text::{Line, Span},
    widgets::{Block, Borders, Row, Table},
    Terminal,
};
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use sysinfo::{System, Networks, Disks, Pid, Signal};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
use tokio::time::sleep;

#[derive(Debug, Clone, Copy, PartialEq)]
enum SortBy {
    Cpu,
    Memory,
    Pid,
}

fn bytes_to_human(b: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    const TB: f64 = GB * 1024.0;
    let bf = b as f64;
    if bf >= TB {
        format!("{:.1} TB", bf / TB)
    } else if bf >= GB {
        format!("{:.1} GB", bf / GB)
    } else if bf >= MB {
        format!("{:.1} MB", bf / MB)
    } else if bf >= KB {
        format!("{:.1} KB", bf / KB)
    } else {
        format!("{} B", b)
    }
}

fn bytes_per_sec_human(bps: f64) -> String {
    if bps.is_nan() || !bps.is_finite() {
        return "0 B/s".to_string();
    }
    let s = bytes_to_human(bps.max(0.0) as u64);
    format!("{}/s", s)
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Initialize system and refresh all data
    let mut sys = System::new_all();
    let mut networks = Networks::new_with_refreshed_list();
    let mut disks = Disks::new_with_refreshed_list();
    sys.refresh_all();

    let cpu_model = sys
        .cpus()
        .first()
        .map_or("Unknown".to_string(), |cpu| cpu.brand().to_string());

    // Track disk I/O rates
    let mut last_proc_read_total: u64 = sys
        .processes()
        .values()
        .map(|p| p.disk_usage().total_read_bytes)
        .sum();
    let mut last_proc_write_total: u64 = sys
        .processes()
        .values()
        .map(|p| p.disk_usage().total_written_bytes)
        .sum();

    // Network: per-interface last totals
    let mut last_net_totals: HashMap<String, (u64, u64)> = networks
        .iter()
        .map(|(name, data)| (name.to_string(), (data.total_received(), data.total_transmitted())))
        .collect();

    let mut last_tick = Instant::now();
    let mut sort_by = SortBy::Cpu;
    let mut selected_process: usize = 0;
    let mut confirmation_pid: Option<Pid> = None;

    loop {
        // Refresh all system information
        sys.refresh_all();
        networks.refresh();
        disks.refresh();

        let now = Instant::now();
        let dt = now.duration_since(last_tick).as_secs_f64().max(1e-9);

        // Compute disk I/O rates
        let proc_read_total: u64 = sys
            .processes()
            .values()
            .map(|p| p.disk_usage().total_read_bytes)
            .sum();
        let proc_write_total: u64 = sys
            .processes()
            .values()
            .map(|p| p.disk_usage().total_written_bytes)
            .sum();

        let disk_read_bps = (proc_read_total.saturating_sub(last_proc_read_total)) as f64 / dt;
        let disk_write_bps = (proc_write_total.saturating_sub(last_proc_write_total)) as f64 / dt;

        last_proc_read_total = proc_read_total;
        last_proc_write_total = proc_write_total;

        // Compute per-interface network rates
        let mut net_rows: Vec<(String, String, String, String, String)> = Vec::new();
        for (name, data) in networks.iter() {
            let (prev_rx, prev_tx) = last_net_totals
                .get(name)
                .cloned()
                .unwrap_or((data.total_received(), data.total_transmitted()));
            let rx = data.total_received();
            let tx = data.total_transmitted();
            let rx_bps = (rx.saturating_sub(prev_rx)) as f64 / dt;
            let tx_bps = (tx.saturating_sub(prev_tx)) as f64 / dt;

            net_rows.push((
                name.to_string(),
                bytes_to_human(rx),
                bytes_to_human(tx),
                bytes_per_sec_human(rx_bps),
                bytes_per_sec_human(tx_bps),
            ));

            last_net_totals.insert(name.to_string(), (rx, tx));
        }

        net_rows.sort_by(|a, b| b.0.cmp(&a.0));

        // Draw UI
        terminal.draw(|f| {
            let size = f.area();

            let outer = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(5),
                    Constraint::Min(8),
                    Constraint::Length(10),
                ])
                .split(size);

            // System info panel
            let sort_label = match sort_by {
                SortBy::Cpu => "CPU",
                SortBy::Memory => "Memory",
                SortBy::Pid => "PID",
            };
            let system_text = vec![
                Line::from(Span::styled(
                    format!("CPU Model: {}", cpu_model),
                    Style::default().fg(Color::Green),
                )),
                Line::from(Span::styled(
                    format!("Total CPU Usage: {:.2}%", sys.global_cpu_info().cpu_usage()),
                    Style::default().fg(Color::Yellow),
                )),
                Line::from(Span::styled(
                    format!("Sort: {} | 'c'=CPU 'm'=Memory 'p'=PID | 'k'=Kill | '?'=Help", sort_label),
                    Style::default().fg(Color::Cyan),
                )),
            ];
            let system_block = Block::default()
                .title("System")
                .borders(Borders::ALL)
                .style(Style::default().fg(Color::White));
            f.render_widget(
                ratatui::widgets::Paragraph::new(system_text).block(system_block),
                outer[0],
            );

            // Processes table
            let mut procs: Vec<_> = sys.processes().values().collect();
            
            match sort_by {
                SortBy::Cpu => {
                    procs.sort_by(|a, b| {
                        b.cpu_usage()
                            .partial_cmp(&a.cpu_usage())
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                }
                SortBy::Memory => {
                    procs.sort_by(|a, b| {
                        b.memory()
                            .partial_cmp(&a.memory())
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                }
                SortBy::Pid => {
                    procs.sort_by(|a, b| a.pid().cmp(&b.pid()));
                }
            }

            let total_mem = sys.total_memory();
            let mut rows: Vec<Row> = procs
                .iter()
                .take(30)
                .enumerate()
                .map(|(idx, p)| {
                    let mem_bytes = p.memory();
                    let mem_pct = (mem_bytes as f64 / total_mem as f64) * 100.0;
                    let status = format!("{:?}", p.status());
                    let row_content = vec![
                        p.name().to_string(),
                        p.pid().to_string(),
                        format!("{:.2}%", p.cpu_usage()),
                        format!("{} ({:.1}%)", bytes_to_human(mem_bytes), mem_pct),
                        status,
                        format!("{}", p.run_time()),
                        "✕".to_string(),
                    ];

                    let style = if idx == selected_process {
                        Style::default().bg(Color::DarkGray).fg(Color::White)
                    } else if p.cpu_usage() > 80.0 {
                        Style::default().fg(Color::Red)
                    } else if p.cpu_usage() > 50.0 {
                        Style::default().fg(Color::Yellow)
                    } else if mem_pct > 20.0 {
                        Style::default().fg(Color::Magenta)
                    } else {
                        Style::default().fg(Color::White)
                    };

                    Row::new(row_content).style(style)
                })
                .collect();

            let table = Table::new(
                rows,
                [
                    Constraint::Percentage(25),
                    Constraint::Percentage(10),
                    Constraint::Percentage(10),
                    Constraint::Percentage(20),
                    Constraint::Percentage(12),
                    Constraint::Percentage(12),
                    Constraint::Length(3),
                ],
            )
            .header(
                Row::new(vec!["Name", "PID", "CPU %", "Memory", "Status", "Runtime", "Kill"])
                    .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
                    .bottom_margin(1),
            )
            .block(Block::default().title("Top Processes").borders(Borders::ALL))
            .style(Style::default().fg(Color::White));

            f.render_widget(table, outer[1]);

            // Bottom stats: Disks | Network
            let bottom = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(outer[2]);

            // Disks panel
            let mut disk_lines: Vec<Line> = vec![
                Line::from(Span::styled(
                    format!(
                        "Aggregate I/O: read {} | write {}",
                        bytes_per_sec_human(disk_read_bps),
                        bytes_per_sec_human(disk_write_bps)
                    ),
                    Style::default().fg(Color::Magenta),
                )),
            ];

            for d in disks.iter().take(4) {
                let mp = d.mount_point().to_string_lossy().to_string();
                let total = bytes_to_human(d.total_space());
                let avail = bytes_to_human(d.available_space());
                disk_lines.push(Line::from(format!("{}  total {}  free {}", mp, total, avail)));
            }

            let disk_block = Block::default().title("Disks").borders(Borders::ALL);
            f.render_widget(
                ratatui::widgets::Paragraph::new(disk_lines).block(disk_block),
                bottom[0],
            );

            // Network panel
            let mut net_table_rows: Vec<Row> = Vec::new();
            for (name, rx_total, tx_total, rx_rate, tx_rate) in net_rows.iter().take(6) {
                net_table_rows.push(Row::new(vec![
                    name.clone(),
                    rx_rate.clone(),
                    tx_rate.clone(),
                    rx_total.clone(),
                    tx_total.clone(),
                ]));
            }

            let net_table = Table::new(
                net_table_rows,
                [
                    Constraint::Percentage(26),
                    Constraint::Percentage(18),
                    Constraint::Percentage(18),
                    Constraint::Percentage(19),
                    Constraint::Percentage(19),
                ],
            )
            .header(
                Row::new(vec!["Iface", "RX/s", "TX/s", "RX total", "TX total"])
                    .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
                    .bottom_margin(1),
            )
            .block(Block::default().title("Network").borders(Borders::ALL));

            f.render_widget(net_table, bottom[1]);

            // Draw confirmation dialog if active
            if let Some(pid) = confirmation_pid {
                let proc_name = sys.process(pid).map(|p| p.name()).unwrap_or("unknown");
                let confirm_text = vec![
                    Line::from(""),
                    Line::from(format!("Kill process: {} (PID: {})?", proc_name, pid)),
                    Line::from(""),
                    Line::from("Press 'y' to confirm or 'n' to cancel"),
                ];
                let popup_block = Block::default()
                    .title("Confirmation")
                    .borders(Borders::ALL)
                    .style(Style::default().bg(Color::DarkGray));
                f.render_widget(
                    ratatui::widgets::Paragraph::new(confirm_text)
                        .block(popup_block)
                        .alignment(ratatui::layout::Alignment::Center),
                    ratatui::layout::Rect {
                        x: size.width / 4,
                        y: size.height / 3,
                        width: size.width / 2,
                        height: 6,
                    },
                );
            }
        })?;

        // Handle input
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Char('c') => sort_by = SortBy::Cpu,
                    KeyCode::Char('m') => sort_by = SortBy::Memory,
                    KeyCode::Char('p') => sort_by = SortBy::Pid,
                    KeyCode::Up => {
                        if selected_process > 0 {
                            selected_process -= 1;
                        }
                    }
                    KeyCode::Down => {
                        if selected_process < 29 {
                            selected_process += 1;
                        }
                    }
                    KeyCode::Char('k') => {
                        // Get the selected process PID
                        let procs: Vec<_> = sys.processes().values().collect();
                        let mut sorted_procs = procs;
                        match sort_by {
                            SortBy::Cpu => {
                                sorted_procs.sort_by(|a, b| {
                                    b.cpu_usage()
                                        .partial_cmp(&a.cpu_usage())
                                        .unwrap_or(std::cmp::Ordering::Equal)
                                });
                            }
                            SortBy::Memory => {
                                sorted_procs.sort_by(|a, b| {
                                    b.memory()
                                        .partial_cmp(&a.memory())
                                        .unwrap_or(std::cmp::Ordering::Equal)
                                });
                            }
                            SortBy::Pid => {
                                sorted_procs.sort_by(|a, b| a.pid().cmp(&b.pid()));
                            }
                        }
                        if let Some(proc) = sorted_procs.get(selected_process) {
                            confirmation_pid = Some(proc.pid());
                        }
                    }
                    KeyCode::Char('y') => {
                        if let Some(pid) = confirmation_pid {
                            let _ = sys.process(pid).map(|p| {
                                p.kill_with(Signal::Term);
                            });
                            confirmation_pid = None;
                        }
                    }
                    KeyCode::Char('n') => {
                        confirmation_pid = None;
                    }
                    KeyCode::Char('?') => {
                        // Help is shown in the header already
                    }
                    _ => {}
                }
            }
        }

        last_tick = now;
        sleep(Duration::from_millis(1000)).await;
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}