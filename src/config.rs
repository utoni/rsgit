use argh::FromArgs;
use std::sync::OnceLock;

#[derive(FromArgs, Debug)]
/// rsgit application command line / environment options
pub struct Args {
    /// http server listen addr/port
    #[argh(option, default = "String::from(\"127.0.0.1:8080\")")]
    pub listen_endpoint: String,

    /// git root directory
    #[argh(option, default = "String::from(\".\")")]
    pub git_root: String,

    /// max syntax highlight bytes
    #[argh(option, default = "512 * 1024")]
    pub max_highlight_bytes: usize,

    /// http max head bytes
    #[argh(option, default = "8 * 1024")]
    pub max_head_bytes: usize,

    /// http_ max body bytes
    #[argh(option, default = "2 * 1024 * 1024")]
    pub max_body_bytes: usize,

    /// http head timeout in seconds
    #[argh(option, default = "30")]
    pub head_timeout: u64,

    /// git clone read refs (from git executable) timeout
    #[argh(option, default = "30")]
    pub git_clone_refs_timeout: u64,

    /// max bytes to diff for the highlighter (on file of a commit)
    #[argh(option, default = "128 * 1024")]
    pub diff_max_bytes: usize,

    /// max bytes to diff for the highlighter (all files of a commit)
    #[argh(option, default = "256 * 1024")]
    pub diff_max_total_bytes: u64,

    /// max files per commit to get processed by the highlighter
    #[argh(option, default = "64")]
    pub diff_max_files: usize,

    /// max blob bytes for a tree item
    #[argh(option, default = "128 * 1024")]
    pub blob_max_bytes: usize,

    /// max log entries shown per page
    #[argh(option, default = "50")]
    pub max_log_entries_per_page: usize,

    /// max ref entries shown per page
    #[argh(option, default = "15")]
    pub max_ref_entries_per_page: usize,
}

static CONFIG: OnceLock<Args> = OnceLock::new();

pub fn init_config() {
    let args: Args = argh::from_env();

    CONFIG.set(args).expect("Config already initialized");
}

pub fn config() -> &'static Args {
    CONFIG.get().expect("Config not initialized")
}
