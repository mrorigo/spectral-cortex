use super::symbol_parser::SymbolParser;
use super::rust::RustSymbolParser;
use super::typescript::TypescriptSymbolParser;
use super::python::PythonSymbolParser;
use std::collections::HashMap;

pub struct ParserRegistry {
    parsers: HashMap<String, Box<dyn SymbolParser>>,
}

impl ParserRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            parsers: HashMap::new(),
        };
        registry.register("rs", Box::new(RustSymbolParser));
        registry.register("ts", Box::new(TypescriptSymbolParser));
        registry.register("tsx", Box::new(TypescriptSymbolParser));
        registry.register("py", Box::new(PythonSymbolParser));
        registry
    }

    pub fn register(&mut self, ext: &str, parser: Box<dyn SymbolParser>) {
        self.parsers.insert(ext.to_string(), parser);
    }

    pub fn get(&self, ext: &str) -> Option<&dyn SymbolParser> {
        self.parsers.get(ext).map(|p| p.as_ref())
    }
}
