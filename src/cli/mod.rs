use clap::{Parser, Subcommand};

pub mod clean_legacy;
pub mod doctor;
pub mod install;
pub mod migrate_v2;
pub mod status;
pub mod uninstall;
pub mod vault;

#[derive(Parser)]
#[command(name = "clipboard-history-mcp", version, about = "Cross-platform clipboard history MCP")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Cmd,
}

#[derive(Subcommand)]
pub enum Cmd {
    /// Run the watcher daemon (poll pbpaste, write to DB)
    Daemon,
    /// Run the MCP stdio server
    Serve,
    /// Install daemon service (launchd on macOS, systemd on Linux)
    Install {
        #[arg(long)]
        window_titles: bool,
        /// Linux only: enable systemd linger so the service survives logout
        #[arg(long)]
        linger: bool,
        /// Auto-mirror non-secret clips to this Obsidian vault path.
        /// Sets `CLIPBOARD_VAULT_PATH` env var on the daemon.
        #[arg(long, value_name = "PATH")]
        vault: Option<std::path::PathBuf>,
    },
    /// Uninstall daemon service
    Uninstall {
        #[arg(long)]
        keep_data: bool,
    },
    /// Show daemon + DB status
    Status,
    /// Doctor diagnostic
    Doctor,
    /// Vault subcommands
    Vault {
        #[arg(value_name = "SUBCMD")]
        sub: String,
        #[arg(value_name = "ID")]
        id: Option<i64>,
    },
    /// Migrate from v2 SQLite (no-op for matching schema)
    MigrateV2,
    /// Remove the legacy v0.3.x macOS data directory
    /// (`~/Library/Application Support/clipboard-history-mcp/`).
    /// No-op on Linux and when the legacy directory does not exist.
    CleanLegacy {
        /// Skip the confirmation prompt and delete immediately.
        #[arg(short, long)]
        yes: bool,
    },
}
