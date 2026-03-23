pub enum AstNodeCategory {
    ApiDefinition,
    Implementation,
    Unknown,
}

pub trait SymbolParser: Send + Sync {
    fn symbol_node_types(&self) -> &[&str];
    fn extract_symbol_id(&self, node: tree_sitter::Node, source: &str) -> Option<String>;
    fn node_category(&self, node: tree_sitter::Node) -> AstNodeCategory;
}
