pub use ted_base::{AlgorithmFactory, AlgorithmType, LowerBoundMethod};
use ted_lb_bib::BinaryBranchAlgorithm;
pub use ted_lb_bib::BinaryBranchFactory;
use ted_lb_label_intersection::LabelIntersectionAlgorithm;
pub use ted_lb_label_intersection::LabelIntersectionFactory;
use ted_lb_sed::SedAlgorithm;
pub use ted_lb_sed::SedFactory;
use ted_lb_sed_struct::StringStructAlgorithm;
pub use ted_lb_sed_struct::StringStructFactory;
pub use ted_lb_structural::{StructuralAlgorithm, StructuralFactory};
pub use tree_parsing::{
    parse_dataset, parse_queries, parse_single, update_label_dict, LabelDict, LabelId, ParsedTree,
};

pub enum Algorithm {
    LabelIntersection(LabelIntersectionAlgorithm),
    Sed(SedAlgorithm),
    StringStruct(StringStructAlgorithm),
    Structural(StructuralAlgorithm),
    BinaryBranch(BinaryBranchAlgorithm),
}

impl From<SedAlgorithm> for Algorithm {
    fn from(algo: SedAlgorithm) -> Self {
        Algorithm::Sed(algo)
    }
}

impl From<StringStructAlgorithm> for Algorithm {
    fn from(algo: StringStructAlgorithm) -> Self {
        Algorithm::StringStruct(algo)
    }
}

impl From<StructuralAlgorithm> for Algorithm {
    fn from(algo: StructuralAlgorithm) -> Self {
        Algorithm::Structural(algo)
    }
}

impl From<LabelIntersectionAlgorithm> for Algorithm {
    fn from(algo: LabelIntersectionAlgorithm) -> Self {
        Algorithm::LabelIntersection(algo)
    }
}

impl From<BinaryBranchAlgorithm> for Algorithm {
    fn from(algo: BinaryBranchAlgorithm) -> Self {
        Algorithm::BinaryBranch(algo)
    }
}

pub fn create_algorithm<F: AlgorithmFactory>() -> Algorithm
where
    // We can now name the concrete type returned by the factory!
    Algorithm: From<F::AlgorithmType>,
{
    // The type of 'algo' is explicitly F::Algorithm
    let algo = F::create_algorithm();

    // The compiler can verify this conversion
    algo.into()
}
