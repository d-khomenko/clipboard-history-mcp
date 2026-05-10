use directories_next::ProjectDirs;
use std::path::PathBuf;

const QUALIFIER: &str = "kz";
const ORG: &str = "me";
const APP: &str = "clipboard-history-mcp";

fn project_dirs() -> ProjectDirs {
    ProjectDirs::from(QUALIFIER, ORG, APP)
        .expect("HOME directory must be set on this OS")
}

pub fn data_dir() -> PathBuf {
    if let Ok(p) = std::env::var("CLIPBOARD_DATA_DIR") {
        return PathBuf::from(p);
    }
    project_dirs().data_dir().to_path_buf()
}

pub fn db_path() -> PathBuf {
    if let Ok(p) = std::env::var("CLIPBOARD_DB_PATH") {
        return PathBuf::from(p);
    }
    data_dir().join("history.db")
}

pub fn pid_file_path() -> PathBuf {
    data_dir().join("daemon.pid")
}

pub fn log_path() -> PathBuf {
    data_dir().join("daemon.log")
}
