use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style, Modifier},
    text::{Line, Span},
    widgets::{Block, Borders, Row, Table},
    Terminal,
};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use sysinfo::{System, Networks, Pid};
#[cfg(feature = "gpu")]
use nvml_wrapper::Nvml;
use std::time::{Duration, Instant};
use std::sync::{Arc, Mutex};
use std::thread;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
enum SortBy {
    Cpu,
    Memory,
    Pid,
}

// Shared data structures
#[derive(Clone)]
struct ProcessInfo {
    name: String,
    pid: Pid,
    cpu_usage: f32,
    memory: u64,
    status: String,
    run_time: u64,
}

struct SharedState {
    processes: Vec<ProcessInfo>,
    cpu_model: String,
    total_cpu_usage: f32,
    total_memory: u64,
    used_memory: u64,
    available_memory: u64,
    total_swap: u64,
    used_swap: u64,
    disk_read_bps: f64,
    disk_write_bps: f64,
    network_data: Vec<(String, String, String, String, String)>,
    paused: bool,
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

fn main() -> std::io::Result<()> {
    // Initialize shared state
    let shared_state = Arc::new(Mutex::new(SharedState {
        processes: Vec::new(),
        cpu_model: String::new(),
        total_cpu_usage: 0.0,
        total_memory: 0,
        used_memory: 0,
        available_memory: 0,
        total_swap: 0,
        used_swap: 0,
        disk_read_bps: 0.0,
        disk_write_bps: 0.0,
        network_data: Vec::new(),
        paused: false,
    }));

    let state_for_process = Arc::clone(&shared_state);
    let state_for_network = Arc::clone(&shared_state);

    // Thread 1: Process monitoring (updates every 1 second)
    thread::spawn(move || {
        let mut sys = System::new_all();
        sys.refresh_all();
        
        // Get CPU model once
        let cpu_model = sys
            .cpus()
            .first()
            .map_or("Unknown".to_string(), |cpu| cpu.brand().to_string());

        let mut last_proc_read_total: u64 = 0;
        let mut last_proc_write_total: u64 = 0;
        let mut last_tick = Instant::now();

        loop {
            // Check if paused
            let is_paused = if let Ok(state) = state_for_process.lock() {
                state.paused
            } else {
                false
            };

            // Only refresh if not paused
            if !is_paused {
                sys.refresh_all();
                
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

                // Collect process information
                let processes: Vec<ProcessInfo> = sys
                    .processes()
                    .values()
                    .map(|p| ProcessInfo {
                        name: p.name().to_string(),
                        pid: p.pid(),
                        cpu_usage: p.cpu_usage(),
                        memory: p.memory(),
                        status: format!("{:?}", p.status()),
                        run_time: p.run_time(),
                    })
                    .collect();

                // Update shared state
                if let Ok(mut state) = state_for_process.lock() {
                    state.processes = processes;
                    state.cpu_model = cpu_model.clone();
                    state.total_cpu_usage = sys.global_cpu_info().cpu_usage();
                    state.total_memory = sys.total_memory();
                    state.used_memory = sys.used_memory();
                    state.available_memory = sys.available_memory();
                    state.total_swap = sys.total_swap();
                    state.used_swap = sys.used_swap();
                    state.disk_read_bps = disk_read_bps;
                    state.disk_write_bps = disk_write_bps;
                }

                last_tick = now;
            }
            
            thread::sleep(Duration::from_millis(1000));
        }
    });

    // Thread 2: Network monitoring (updates every 1 second)
    thread::spawn(move || {
        let mut networks = Networks::new_with_refreshed_list();
        let mut last_net_totals: HashMap<String, (u64, u64)> = networks
            .iter()
            .map(|(name, data)| (name.to_string(), (data.total_received(), data.total_transmitted())))
            .collect();
        let mut last_tick = Instant::now();

        loop {
            // Check if paused
            let is_paused = if let Ok(state) = state_for_network.lock() {
                state.paused
            } else {
                false
            };

            // Only refresh if not paused
            if !is_paused {
                networks.refresh();
                
                let now = Instant::now();
                let dt = now.duration_since(last_tick).as_secs_f64().max(1e-9);

                let mut net_rows: Vec<(String, String, String, String, String)> = Vec::new();
                for (name, data) in networks.iter() {
                    // Filter: exclude only specific virtual/loopback interfaces
                    let name_lower = name.to_lowercase();
                    let should_exclude = name_lower.contains("npcap")
                        || name_lower.contains("nocap")
                        || name_lower.starts_with("lo")
                        || name_lower.starts_with("docker")
                        || name_lower.starts_with("veth")
                        || name_lower.starts_with("br-")
                        || name_lower.starts_with("vir");
                    
                    if should_exclude {
                        continue;
                    }
                    
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

                // Update shared state
                if let Ok(mut state) = state_for_network.lock() {
                    state.network_data = net_rows;
                }

                last_tick = now;
            }
            
            thread::sleep(Duration::from_millis(1000));
        }
    });

    // Main thread: UI and input handling
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let mut sort_by = SortBy::Cpu;
    let mut command_input = String::new();
    let mut command_mode = false;
    let mut command_output: Vec<String> = Vec::new();

    // Store a local copy of system state for process detail lookups
    let mut local_sys = System::new_all();
    let mut last_sys_refresh = Instant::now();
    let mut last_ui_update = Instant::now();

    // Initialize NVML for GPU monitoring (if enabled)
    #[cfg(feature = "gpu")]
    let nvml = Nvml::init().ok(); // Handle initialization failure gracefully

    loop {
        // Refresh local system occasionally for command lookups (every 2 seconds)
        if last_sys_refresh.elapsed() > Duration::from_secs(2) {
            local_sys.refresh_all();
            last_sys_refresh = Instant::now();
        }

        // Only redraw if enough time has passed (throttle UI updates)
        let ui_update_interval = if command_mode {
            Duration::from_millis(16) // Fast updates when typing
        } else {
            Duration::from_millis(100) // Slower updates in normal mode
        };

        let should_update_ui = last_ui_update.elapsed() >= ui_update_interval;
        
        // Handle input with timeout
        let poll_timeout = Duration::from_millis(50);
        
        if event::poll(poll_timeout)? {
            if let Event::Key(key) = event::read()? {
                // Only process key press events, not release or repeat
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                
                if command_mode {
                    // Command mode input handling
                    match key.code {
                        KeyCode::Char(c) => {
                            command_input.push(c);
                        }
                        KeyCode::Backspace => {
                            command_input.pop();
                        }
                        KeyCode::Enter => {
                            // Process the command
                            let cmd = command_input.trim().to_string();
                            command_output.clear();
                            
                            if cmd.starts_with("p ") || cmd.starts_with("P ") {
                                // Parse PID and show process details
                                let pid_str = cmd[2..].trim();
                                if let Ok(pid_num) = pid_str.parse::<usize>() {
                                    let pid = Pid::from(pid_num);
                                    // Refresh local system to get latest process info
                                    local_sys.refresh_all();
                                    if let Some(proc) = local_sys.process(pid) {
                                        command_output.push(format!("Process Details for PID {}:", pid_num));
                                        command_output.push(format!("  Name: {}", proc.name()));
                                        command_output.push(format!("  Status: {:?}", proc.status()));
                                        command_output.push(format!("  CPU Usage: {:.2}%", proc.cpu_usage()));
                                        command_output.push(format!("  Memory: {}", bytes_to_human(proc.memory())));
                                        command_output.push(format!("  Virtual Memory: {}", bytes_to_human(proc.virtual_memory())));
                                        command_output.push(format!("  Runtime: {} seconds", proc.run_time()));
                                        command_output.push(format!("  Disk Read: {}", bytes_to_human(proc.disk_usage().total_read_bytes)));
                                        command_output.push(format!("  Disk Write: {}", bytes_to_human(proc.disk_usage().total_written_bytes)));
                                        if let Some(cwd) = proc.cwd() {
                                            command_output.push(format!("  CWD: {}", cwd.display()));
                                        }
                                        if let Some(exe) = proc.exe() {
                                            command_output.push(format!("  Executable: {}", exe.display()));
                                        }
                                        last_sys_refresh = Instant::now();
                                    } else {
                                        command_output.push(format!("Process with PID {} not found", pid_num));
                                    }
                                } else {
                                    command_output.push("Invalid PID format. Usage: p <PID>".to_string());
                                }
                            } else if cmd == "help" || cmd == "?" {
                                command_output.push("Available commands:".to_string());
                                command_output.push("  p <PID> - Show detailed process information".to_string());
                                command_output.push("  help or ? - Show this help message".to_string());
                                command_output.push("  Press ESC to exit command mode".to_string());
                            } else if !cmd.is_empty() {
                                command_output.push(format!("Unknown command: '{}'. Type 'help' for available commands.", cmd));
                            }
                            
                            command_input.clear();
                            command_mode = false;
                        }
                        KeyCode::Esc => {
                            command_input.clear();
                            command_mode = false;
                        }
                        _ => {}
                    }
                } else {
                    // Normal mode input handling
                    match key.code {
                        KeyCode::Char(':') => {
                            command_mode = true;
                            command_input.clear();
                        }
                        KeyCode::Char('q') => break,
                        KeyCode::Char('c') => sort_by = SortBy::Cpu,
                        KeyCode::Char('m') => sort_by = SortBy::Memory,
                        KeyCode::Char('p') => sort_by = SortBy::Pid,
                        KeyCode::Char(' ') | KeyCode::Char('s') => {
                            // Toggle pause with spacebar or 's'
                            if let Ok(mut state) = shared_state.lock() {
                                state.paused = !state.paused;
                            }
                        }
                        _ => {}
                    }
                }
                // Force UI update after input
                last_ui_update = Instant::now().checked_sub(ui_update_interval).unwrap_or(Instant::now());
            }
        }

        // Skip rendering if not enough time has passed
        if !should_update_ui {
            continue;
        }

        last_ui_update = Instant::now();

        // Draw UI
        terminal.draw(|f| {
            let size = f.area();

            // Get shared state
            let state = shared_state.lock().unwrap();

            let outer = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(5),
                    Constraint::Min(8),
                    Constraint::Length(10),
                    Constraint::Length(8),
                ])
                .split(size);

            // System info panel
            let sort_label = match sort_by {
                SortBy::Cpu => "CPU",
                SortBy::Memory => "Memory",
                SortBy::Pid => "PID",
            };
            let pause_status = if state.paused { " [PAUSED]" } else { "" };
            let mut system_text = vec![
                Line::from(Span::styled(
                    format!("CPU Model: {}", state.cpu_model),
                    Style::default().fg(Color::Green),
                )),
                Line::from(Span::styled(
                    format!("Total CPU Usage: {:.2}%{}", state.total_cpu_usage, pause_status),
                    if state.paused {
                        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::Yellow)
                    },
                )),
                Line::from(Span::styled(
                    format!("Sort: {} | 'c'=CPU 'm'=Memory 'p'=PID | Space/s=Pause | ':'=Cmd", sort_label),
                    Style::default().fg(Color::Cyan),
                )),
                Line::from(Span::styled(
                    format!(
                        "RAM: {}/{} ({:.2}%)",
                        state.used_memory / 1024, // Convert KB to MB
                        state.total_memory / 1024,
                        (state.used_memory as f64 / state.total_memory as f64) * 100.0
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
                outer[0],
            );

            // Processes table
            let mut procs = state.processes.clone();
            
            match sort_by {
                SortBy::Cpu => {
                    procs.sort_by(|a, b| {
                        b.cpu_usage
                            .partial_cmp(&a.cpu_usage)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                }
                SortBy::Memory => {
                    procs.sort_by(|a, b| {
                        b.memory
                            .partial_cmp(&a.memory)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                }
                SortBy::Pid => {
                    procs.sort_by(|a, b| a.pid.cmp(&b.pid));
                }
            }

            let total_mem = state.total_memory;
            let rows: Vec<Row> = procs
                .iter()
                .take(30)
                .map(|p| {
                    let mem_bytes = p.memory;
                    let mem_pct = (mem_bytes as f64 / total_mem as f64) * 100.0;
                    let row_content = vec![
                        p.name.clone(),
                        p.pid.to_string(),
                        format!("{:.2}%", p.cpu_usage),
                        format!("{} ({:.1}%)", bytes_to_human(mem_bytes), mem_pct),
                        p.status.clone(),
                        format!("{}", p.run_time),
                    ];

                    let style = if p.cpu_usage > 80.0 {
                        Style::default().fg(Color::Red)
                    } else if p.cpu_usage > 50.0 {
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
                    Constraint::Percentage(12),
                    Constraint::Percentage(12),
                    Constraint::Percentage(23),
                    Constraint::Percentage(13),
                    Constraint::Percentage(15),
                ],
            )
            .header(
                Row::new(vec!["Name", "PID", "CPU %", "Memory", "Status", "Runtime"])
                    .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
                    .bottom_margin(1),
            )
            .block(Block::default().title("Top Processes").borders(Borders::ALL))
            .style(Style::default().fg(Color::White));

            f.render_widget(table, outer[1]);

            // Bottom stats: RAM | Network
            let bottom = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(outer[2]);

            // RAM panel
            let total_mem = state.total_memory;
            let used_mem = state.used_memory;
            let available_mem = state.available_memory;
            let mem_percent = if total_mem > 0 {
                (used_mem as f64 / total_mem as f64) * 100.0
            } else {
                0.0
            };

            let total_swap = state.total_swap;
            let used_swap = state.used_swap;
            let swap_percent = if total_swap > 0 {
                (used_swap as f64 / total_swap as f64) * 100.0
            } else {
                0.0
            };

            let mut ram_lines: Vec<Line> = vec![
                Line::from(Span::styled(
                    format!("RAM: {} / {} ({:.1}%)", 
                        bytes_to_human(used_mem),
                        bytes_to_human(total_mem),
                        mem_percent
                    ),
                    if mem_percent > 90.0 {
                        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
                    } else if mem_percent > 75.0 {
                        Style::default().fg(Color::Yellow)
                    } else {
                        Style::default().fg(Color::Green)
                    },
                )),
                Line::from(Span::styled(
                    format!("Available: {}", bytes_to_human(available_mem)),
                    Style::default().fg(Color::Cyan),
                )),
            ];

            // Add swap info if swap exists
            if total_swap > 0 {
                ram_lines.push(Line::from(""));
                ram_lines.push(Line::from(Span::styled(
                    format!("Swap: {} / {} ({:.1}%)", 
                        bytes_to_human(used_swap),
                        bytes_to_human(total_swap),
                        swap_percent
                    ),
                    if swap_percent > 75.0 {
                        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
                    } else if swap_percent > 50.0 {
                        Style::default().fg(Color::Yellow)
                    } else {
                        Style::default().fg(Color::Cyan)
                    },
                )));
            } else {
                ram_lines.push(Line::from(""));
                ram_lines.push(Line::from(Span::styled(
                    "Swap: Not configured",
                    Style::default().fg(Color::DarkGray),
                )));
            }

            // Add disk I/O info
            ram_lines.push(Line::from(""));
            ram_lines.push(Line::from(Span::styled(
                format!("Disk I/O: ↓{} ↑{}", 
                    bytes_per_sec_human(state.disk_read_bps),
                    bytes_per_sec_human(state.disk_write_bps)
                ),
                Style::default().fg(Color::Magenta),
            )));

            let ram_block = Block::default().title("Memory").borders(Borders::ALL);
            f.render_widget(
                ratatui::widgets::Paragraph::new(ram_lines).block(ram_block),
                bottom[0],
            );

            // Network panel
            let mut net_table_rows: Vec<Row> = Vec::new();
            for (name, rx_total, tx_total, rx_rate, tx_rate) in state.network_data.iter().take(6) {
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

            // Command Line panel
            let cmd_prompt = if command_mode {
                format!("> {}_", command_input)
            } else {
                "> (Press ':' to enter command mode, 'p <PID>' for process details)".to_string()
            };
            
            let mut cmd_lines = vec![
                Line::from(Span::styled(
                    cmd_prompt,
                    if command_mode {
                        Style::default().fg(Color::Green)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    },
                )),
            ];
            
            // Show command output
            for output_line in command_output.iter().rev().take(5).rev() {
                cmd_lines.push(Line::from(Span::styled(
                    output_line.clone(),
                    Style::default().fg(Color::Yellow),
                )));
            }
            
            let cmd_block = Block::default()
                .title("Command Line")
                .borders(Borders::ALL)
                .style(Style::default().fg(Color::White));
            f.render_widget(
                ratatui::widgets::Paragraph::new(cmd_lines).block(cmd_block),
                outer[3],
            );
        })?;
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}