use tree_parsing::ParsedTree;

/// Trait for lower bound algorithm methods
pub trait LowerBoundMethod {
    /// Name/identifier for this method
    const NAME: &'static str;

    /// Constant indicating whether the method supports an index
    const SUPPORTS_INDEX: bool;

    /// Type of preprocessed data if needed
    /// If no preprocessing is needed, this can be `()`
    type PreprocessedDataType;

    /// Type of index if supported
    /// If `SUPPORTS_INDEX` is false, this can be `()`
    type IndexType;

    /// Preprocess the data before computing lower bound
    fn preprocess(&mut self, data: &[ParsedTree]) -> Result<Self::PreprocessedDataType, String>;

    /// Compute the lower bound
    fn lower_bound(query: &Self::PreprocessedDataType, data: &Self::PreprocessedDataType) -> usize;

    fn build_index(data: &[Self::PreprocessedDataType]) -> Result<Self::IndexType, String>;

    /// Query the index with the preprocessed query data
    fn query_index(
        query: &Self::PreprocessedDataType,
        index: &Self::IndexType,
    ) -> Vec<(usize, usize)>;
}

/// Enum to identify different algorithm types
#[derive(Debug, Clone, PartialEq)]
pub enum AlgorithmType {
    Sed,
    SedStruct,
    Structural,
    BinaryBranch,
    LabelIntersection,
    Histogram,
}

/// Factory trait for creating algorithm instances
pub trait AlgorithmFactory {
    fn create_algorithm(&self, algo_type: AlgorithmType) -> impl LowerBoundMethod;
}
