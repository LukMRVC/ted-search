use clap::{Parser, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum LowerBoundMethods {
    /// Histogram lower bound
    Hist,
    /// Label intersection lower bound
    Lblint,
    /// String edit distance lower bound
    Sed,
    /// String edit distance with structure lower bound
    SEDStruct,
    /// Structural filter lower bound
    Structural,
    /// Structural variant filter lower bound
    StructuralVariant,
    /// Binary branch lower bound
    Bib,
}

#[derive(Parser)]
#[command(name = "TED Search CLI")]
#[command(version = "0.1.0")]
#[command(about = "A command-line interface for TED search algorithms", long_about = None)]
pub struct Cli {
    /// Path to the dataset file
    #[arg(short, long)]
    pub dataset: PathBuf,

    /// Path to the queries file in CSV format <threshold>;<tree>
    #[arg(short, long)]
    pub queries: PathBuf,

    /// Run using this lower bound method
    #[arg(value_enum)]
    pub method: LowerBoundMethods,

    /// Total number of runs for each method
    /// Then the lowest duration of all runs is taken as result
    #[arg(long = "runs", short = 'r', default_value_t = 1)]
    pub runs: usize,

    /// CSV query delimiter
    #[clap(long, default_value_t = ';')]
    pub delimiter: char,
}
