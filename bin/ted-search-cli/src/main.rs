use clap::Parser;
use ted_search::{
    create_algorithm, AlgorithmFactory, BinaryBranchFactory, LabelIntersectionFactory, SedFactory,
    StringStructFactory, StructuralFactory,
};

mod cli;

use cli::Cli;

fn main() {
    let cli = Cli::parse();

    let lower_bound_method = match cli.method {
        cli::LowerBoundMethods::Lblint => create_algorithm::<LabelIntersectionFactory>(),
        cli::LowerBoundMethods::Sed => create_algorithm::<SedFactory>(),
        cli::LowerBoundMethods::SEDStruct => create_algorithm::<StringStructFactory>(),
        cli::LowerBoundMethods::Structural => create_algorithm::<StructuralFactory>(),
        cli::LowerBoundMethods::Bib => create_algorithm::<BinaryBranchFactory>(),
        _ => unimplemented!("Histogram method is not implemented yet"),
    };
}
