use tree_sitter::Node;
use crate::ast::symbol_parser::{SymbolParser, AstNodeCategory};

pub struct TypescriptSymbolParser;

impl SymbolParser for TypescriptSymbolParser {
    fn symbol_node_types(&self) -> &[&str] {
        &["function_declaration", "class_declaration", "interface_declaration", "method_definition"]
    }

    fn extract_symbol_id(&self, node: Node, source: &str) -> Option<String> {
        let name_child = node.child_by_field_name("name")?;
        let name = name_child.utf8_text(source.as_bytes()).ok()?;
        Some(format!("{}:{}", node.kind(), name))
    }

    fn node_category(&self, node: Node) -> AstNodeCategory {
        match node.kind() {
            "interface_declaration" => AstNodeCategory::ApiDefinition,
            _ => AstNodeCategory::Implementation,
        }
    }
}
