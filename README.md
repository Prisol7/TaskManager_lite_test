# 🚀 TaskManager with rust

A lightweight, real-time system monitor built in Rust with a sleek terminal interface. Because I needed a project to do.

![Rust](https://img.shields.io/badge/rust-%23000000.svg?style=for-the-badge&logo=rust&logoColor=white)
![License](https://img.shields.io/badge/license-MIT-blue.svg?style=for-the-badge)

## ✨ What is this?

Ever opened Task Manager and thought "this is cool, but what if it was lighter and in rust" Well, here it is. This is a real-time system monitoring tool that shows you everything happening on your machine without leaving your terminal. 

It's like `htop`, but with more colors and in rust(idk what language htop uses). 

## 🎯 Features

- **📊 Real-time Process Monitoring** - See what's running, how much CPU/Memory it's using, and more
- **🔄 Multi-threaded Architecture** - Separate threads for processes and network monitoring so nothing blocks
- **🌐 Network Stats** - Track bandwidth usage across your network interfaces (WiFi, Ethernet)
- **💾 Memory Tracking** - Keep an eye on RAM and Swap usage with color-coded warnings
- **⏸️ Pause/Resume** - Freeze the display when you need to examine something closely
- **🎨 Color-coded Interface** - Red for high usage, yellow for warnings, green for "we're good"
- **⌨️ Interactive Commands** - Type `p <PID>` to get detailed info on any process
- **📈 Sortable Views** - Sort by CPU, Memory, or PID with a single keypress
- **💨 Disk I/O Tracking** - Monitor read/write speeds in real-time

## 🛠️ Built With

- **[Rust](https://www.rust-lang.org/)** - Because I am a progamming hipster
- **[ratatui](https://github.com/ratatui-org/ratatui)** - For that TUI
- **[sysinfo](https://github.com/GuillaumeGomez/sysinfo)** - System information library
- **[crossterm](https://github.com/crossterm-rs/crossterm)** - Cross-platform terminal 

## 📸 Example Images

will add image as soon as I find out how in markdown

## 🚀 Getting Started

### Prerequisites

- Rust - [Install here](https://rustup.rs/)

### Installation

```bash
# Clone the repo
git clone https://github.com/Prisol7/TaskManager_lite_test.git
cd TaskManager_lite_test

# Build it
cargo build

# Run it
cargo run
```

Or just run it directly:
```bash
cargo run
```

## 🎮 How to Use

### Basic Controls

| Key | What it does |
|-----|-------------|
| `q` | Quit (obviously) |
| `c` | Sort by CPU usage |
| `m` | Sort by Memory usage |
| `p` | Sort by PID |
| `Space` or `s` | Pause/Resume monitoring |
| `:` | Enter command mode |

### Command Mode

Press `:` to enter command mode, then try these:

- `p <PID>` - Show detailed info about a process (e.g., `p 1234`)
- `help` or `?` - Show available commands
- `ESC` - Exit command mode

## 🏗️ Architecture

The app uses a multi-threaded design to keep things snappy:

```
┌─────────────────────────────────────┐
│         Main Thread (UI)            │
│  • Handles keyboard input           │
│  • Renders the interface            │
│  • Processes commands               │
└─────────────────────────────────────┘
            ↕️ Shared State (Arc<Mutex>)
┌──────────────────┬──────────────────┐
│ Process Monitor  │ Network Monitor  │
│ • CPU tracking   │ • Bandwidth      │
│ • Memory stats   │ • Interface data │
│ • Disk I/O       │ • TX/RX rates    │
│ Updates: 1s      │ Updates: 1s      │
└──────────────────┴──────────────────┘
```

Each monitoring thread runs independently and updates the shared state, while the main thread handles all user interaction and rendering. This means the UI never freezes, even when collecting system stats.

## 🎨 Color Coding

Because colors make everything better:

- 🟢 **Green** - Everything's chill (< 75% usage)
- 🟡 **Yellow** - Getting warm (75-90% usage)
- 🔴 **Red** - Things are bad(> 90% usage)
- 🔵 **Cyan** - Info and headers
- 🟣 **Magenta** - Disk I/O and special metrics

## 🤔 Why I Built This

Honestly? I wanted to learn Rust's and this was a cool idea I thought of when i was using task manager. It is pretty simple but for now I like it. I do got more things planned. I wanted to add functions to kill processes and packet sniffing but I got school so it will be slow.

Also, I wanted to use tauri to make a well designed gui but that will have to wait, terminal based UI is good for now.


## 🐛 What to fix

- Network section could be better
- Add a way to capture Processes into a file
- Process history
- More commands

## 📄 License

MIT - Do whatever you want with it, just not my fault if your computer breaks

## 🙏 Acknowledgments

- The Rust community
- Coffee
- Claude for helping me learn markdown ... give me free pro ....

## 💬 Contact

Found a bug? Have a suggestion?

Fork and fix it yourself :)


Made with Luv

*P.S. - Yes, I know there are better task managers out there. But I am a literal student who is just learing rust*
