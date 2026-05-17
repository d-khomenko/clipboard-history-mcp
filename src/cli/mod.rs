use clap::{Parser, Subcommand};

pub mod clean_legacy;
pub mod doctor;
pub mod install;
pub mod migrate_v2;
pub mod status;
pub mod uninstall;
pub mod vault;
pub mod store_helper;
pub mod copy_cmd;
pub mod pin_cmd;
pub mod unpin_cmd;
pub mod delete_cmd;
pub mod clear_cmd;
pub mod unlock_secret_cmd;
pub mod audit;

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
    /// Restore a clip to the system pasteboard. Used by Klipta's `CopyAction`.
    Copy {
        #[arg(value_name = "ID")]
        id: i64,
    },
    /// Pin a clip.
    Pin {
        #[arg(value_name = "ID")]
        id: i64,
    },
    /// Unpin a clip.
    Unpin {
        #[arg(value_name = "ID")]
        id: i64,
    },
    /// Hard-delete a clip and its secrets row if applicable.
    Delete {
        #[arg(value_name = "ID")]
        id: i64,
    },
    /// Clear history. Scope: `all` | `older-than-days:N` | `kind:K`.
    Clear {
        #[arg(long, value_name = "SCOPE")]
        scope: String,
        /// Skip the confirmation prompt.
        #[arg(short, long)]
        yes: bool,
    },
    /// Decrypt and emit a stored secret on stdout. Touch ID gated.
    UnlockSecret {
        #[arg(value_name = "ID")]
        id: i64,
        #[arg(long, value_name = "REASON")]
        reason: String,
    },
    /// Emit recent audit-log entries as JSON. Used by Klipta's `AuditPanel`.
    Audit(crate::cli::audit::AuditArgs),
}
