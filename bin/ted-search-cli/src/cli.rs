use clap::{Parser, ValueEnum};
use std::path::PathBuf;
use ted_search::{
    create_algorithm, create_sed_algorithm, create_sed_exact_algorithm,
    create_sed_struct_algorithm, Algorithm, BinaryBranchFactory, LabelIntersectionFactory,
    StructuralFactory, TraversalKind,
};

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum LowerBoundMethods {
    /// Label intersection lower bound
    Lblint,
    /// String edit distance lower bound
    Sed,
    /// Exact string edit distance lower bound
    SedExact,
    /// String edit distance with structure lower bound
    SEDStruct,
    /// Structural filter lower bound
    Structural,
    /// Binary branch lower bound
    Bib,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum TraversalOption {
    Preorder,
    Postorder,
    ReversedPreorder,
    ReversedPostorder,
}

impl From<TraversalOption> for TraversalKind {
    fn from(value: TraversalOption) -> Self {
        match value {
            TraversalOption::Preorder => TraversalKind::Preorder,
            TraversalOption::Postorder => TraversalKind::Postorder,
            TraversalOption::ReversedPreorder => TraversalKind::ReversedPreorder,
            TraversalOption::ReversedPostorder => TraversalKind::ReversedPostorder,
        }
    }
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

    /// First traversal used by sed, sed-exact, and sed-struct
    #[arg(long = "sed-traversal-first", value_enum, default_value_t = TraversalOption::Preorder)]
    pub sed_traversal_first: TraversalOption,

    /// Second traversal used by sed, sed-exact, and sed-struct
    #[arg(long = "sed-traversal-second", value_enum, default_value_t = TraversalOption::Postorder)]
    pub sed_traversal_second: TraversalOption,

    /// Total number of runs for each method
    /// Then the lowest duration of all runs is taken as result
    #[arg(long = "runs", short = 'r', default_value_t = 1)]
    pub runs: usize,

    /// CSV query delimiter
    #[clap(long, default_value_t = ';')]
    pub delimiter: char,
}

impl Cli {
    pub fn algorithm(&self) -> Algorithm {
        let traversal_a: TraversalKind = self.sed_traversal_first.into();
        let traversal_b: TraversalKind = self.sed_traversal_second.into();

        match self.method {
            LowerBoundMethods::Lblint => create_algorithm::<LabelIntersectionFactory>(),
            LowerBoundMethods::Sed => create_sed_algorithm(traversal_a, traversal_b),
            LowerBoundMethods::SedExact => create_sed_exact_algorithm(traversal_a, traversal_b),
            LowerBoundMethods::SEDStruct => create_sed_struct_algorithm(traversal_a, traversal_b),
            LowerBoundMethods::Structural => create_algorithm::<StructuralFactory>(),
            LowerBoundMethods::Bib => create_algorithm::<BinaryBranchFactory>(),
        }
    }
}
