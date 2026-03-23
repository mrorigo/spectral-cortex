use tree_sitter::Node;
use crate::ast::symbol_parser::{SymbolParser, AstNodeCategory};

pub struct PythonSymbolParser;

impl SymbolParser for PythonSymbolParser {
    fn symbol_node_types(&self) -> &[&str] {
        &["function_definition", "class_definition"]
    }

    fn extract_symbol_id(&self, node: Node, source: &str) -> Option<String> {
        let name_child = node.child_by_field_name("name")?;
        let name = name_child.utf8_text(source.as_bytes()).ok()?;
        Some(format!("{}:{}", node.kind(), name))
    }

    fn node_category(&self, _node: Node) -> AstNodeCategory {
        // Python doesn't have a direct "interface" keyword in the AST, 
        // but we can potentially use abc.ABC in the future.
        AstNodeCategory::Implementation
    }
}
