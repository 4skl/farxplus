use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(author, version, about = "FARxPlus: Advanced Sims 1 .far archive manager")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Extract a .far archive to a directory
    Extract {
        #[arg(short, long)]
        input: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Pack a directory into a new .far archive
    Pack {
        #[arg(short, long)]
        input_dir: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
    },
}