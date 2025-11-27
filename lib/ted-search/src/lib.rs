pub use ted_base::{AlgorithmFactory, AlgorithmType, LowerBoundMethod};
pub use ted_lb_bib::BinaryBranchLowerBoundFactory;
pub use ted_lb_label_intersection::LabelIntersectionFactory;
pub use ted_lb_sed::SedFactory;
pub use ted_lb_sed_struct::StringStructFactory;
pub use ted_lb_structural::StructuralLowerBoundFactory;
pub use tree_parsing::{
    parse_dataset, parse_queries, parse_single, update_label_dict, LabelDict, LabelId, ParsedTree,
};

pub fn create_method<AF: AlgorithmFactory>() -> impl LowerBoundMethod {
    AF::create_algorithm()
}
