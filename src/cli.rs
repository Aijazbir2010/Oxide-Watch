use clap::{Parser, builder::TypedValueParser};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    version,
    about,
    long_about = None,
    override_usage = "oxide_watch [OPTIONS] --binary <BINARY> -- [ARGS]..."
)]
pub(crate) struct CliArgs {
    // Path to the binary user wants to run
    #[arg(short, long, help = "Path for the target binary to run.")]
    pub(crate) binary: PathBuf,

    // Our Own Program's Options
    // ---
    // Maximum wall time allowed (in milliseconds)
    #[arg(
        short,
        long,
        default_value_t = 5000,
        value_parser = clap::value_parser!(u64).range(50..=60000),
        help = "Maximum time allowed for the binary for execution (in ms). [MIN: 50 and MAX: 60000]"
    )]
    pub(crate) timeout: u64,
    
    // Maximum memory limit for the binary (in MB)
    #[arg(
        short,
        long,
        default_value_t = 256,
        value_parser = clap::value_parser!(u64).range(128..=4096).map(|v| v as usize),
        help = "Maximum memory usage limit for the binary (in MB). [MIN: 128 and MAX: 4096]"
    )]
    pub(crate) memory: usize,

    // Export flag (if enabled, we output a JSON to report usage)
    #[arg(
        short,
        long,
        help = "A JSON file is exported that reports the resource usage by the process in detail."
    )]
    pub(crate) export: bool,

    // ---
    // Arguments to pass to the target binary
    #[arg(
        trailing_var_arg = true,
        allow_hyphen_values = true,
        help = "Arguments which need to specified to the target binary."
    )]
    pub(crate) args: Vec<String>,
}