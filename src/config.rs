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
}

static CONFIG: OnceLock<Args> = OnceLock::new();

pub fn init_config() {
    let args: Args = argh::from_env();

    CONFIG.set(args).expect("Config already initialized");
}

pub fn config() -> &'static Args {
    CONFIG.get().expect("Config not initialized")
}
