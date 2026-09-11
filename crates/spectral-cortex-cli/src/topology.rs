use anyhow::{Context, Result};
use git2::{ObjectType, Repository, TreeWalkMode, TreeWalkResult};
use spectral_cortex::model::smg_note::{
    TopologyEdge, TopologyRelation, TopologySnapshot, TopologySymbol,
};
use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::ast::registry::ParserRegistry;
use crate::ast::symbol_parser::SymbolParser;

/// Build complete code-topology snapshots for selected repository commits.
pub(crate) fn collect_snapshots(
    repo_path: &Path,
    commit_ids: impl IntoIterator<Item = String>,
    registry: &ParserRegistry,
) -> Result<Vec<TopologySnapshot>> {
    let repo = Repository::open(repo_path).with_context(|| "opening repository for topology")?;
    commit_ids
        .into_iter()
        .map(|id| build_snapshot(&repo, &id, registry))
        .collect()
}

fn build_snapshot(
    repo: &Repository,
    commit_id: &str,
    registry: &ParserRegistry,
) -> Result<TopologySnapshot> {
    let commit = repo
        .revparse_single(commit_id)?
        .peel_to_commit()
        .with_context(|| format!("loading topology commit {commit_id}"))?;
    let timestamp = commit.time().seconds().max(0) as u64;
    let tree = commit.tree()?;
    let mut files = Vec::new();
    tree.walk(TreeWalkMode::PreOrder, |root, entry| {
        let Some(name) = entry.name() else {
            return TreeWalkResult::Ok;
        };
        let path = format!("{root}{name}");
        let Some(ext) = Path::new(name).extension().and_then(|e| e.to_str()) else {
            return TreeWalkResult::Ok;
        };
        if registry.get(ext).is_some() && entry.kind() == Some(ObjectType::Blob) {
            files.push((path, entry.id()));
        }
        TreeWalkResult::Ok
    })?;

    let mut symbols = Vec::new();
    let mut calls = Vec::new();
    let mut imports = Vec::new();
    let mut file_symbols: HashMap<String, Vec<String>> = HashMap::new();
    for (path, blob_id) in files {
        let Some(ext) = Path::new(&path).extension().and_then(|e| e.to_str()) else {
            continue;
        };
        let Some(parser) = registry.get(ext) else {
            continue;
        };
        let source = String::from_utf8_lossy(repo.find_blob(blob_id)?.content()).to_string();
        let mut ts = tree_sitter::Parser::new();
        let language = match ext {
            "rs" => tree_sitter_rust::LANGUAGE,
            "py" => tree_sitter_python::LANGUAGE,
            "js" | "jsx" => tree_sitter_javascript::LANGUAGE,
            "ts" | "tsx" => tree_sitter_typescript::LANGUAGE_TYPESCRIPT,
            _ => continue,
        };
        ts.set_language(&language.into())
            .map_err(|e| anyhow::anyhow!("setting {ext} parser: {e}"))?;
        let Some(ast) = ts.parse(source.as_bytes(), None) else {
            continue;
        };
        let mut declarations: HashMap<String, Vec<String>> = HashMap::new();
        collect_declarations(
            ast.root_node(),
            &source,
            parser,
            &path,
            &mut declarations,
            &mut symbols,
        );
        collect_calls(ast.root_node(), &source, parser, &declarations, &mut calls);
        let symbols_in_file: Vec<String> = declarations.values().flatten().cloned().collect();
        file_symbols.insert(path.clone(), symbols_in_file);
        collect_imports(ast.root_node(), &source, &path, &mut imports);
    }

    let symbol_ids: HashSet<_> = symbols.iter().map(|s| s.symbol_id.as_str()).collect();
    let mut edges = HashSet::new();
    for (source, target) in calls {
        if source != target
            && symbol_ids.contains(source.as_str())
            && symbol_ids.contains(target.as_str())
        {
            edges.insert(TopologyEdge {
                source_symbol_id: source,
                target_symbol_id: target,
                relation: TopologyRelation::Call,
            });
        }
    }
    for (source_file, import_text) in imports {
        let Some(target_file) = resolve_import_path(&source_file, &import_text, &file_symbols)
        else {
            continue;
        };
        for source in file_symbols.get(&source_file).into_iter().flatten() {
            for target in file_symbols.get(&target_file).into_iter().flatten() {
                if source != target {
                    edges.insert(TopologyEdge {
                        source_symbol_id: source.clone(),
                        target_symbol_id: target.clone(),
                        relation: TopologyRelation::Import,
                    });
                }
            }
        }
    }
    symbols.sort_by(|a, b| a.symbol_id.cmp(&b.symbol_id));
    let mut edges: Vec<_> = edges.into_iter().collect();
    edges.sort_by(|a, b| {
        (&a.source_symbol_id, &a.target_symbol_id).cmp(&(&b.source_symbol_id, &b.target_symbol_id))
    });
    Ok(TopologySnapshot {
        commit_id: commit_id.to_string(),
        timestamp,
        symbols,
        edges,
    })
}

fn collect_imports(
    node: tree_sitter::Node,
    source: &str,
    path: &str,
    imports: &mut Vec<(String, String)>,
) {
    if matches!(
        node.kind(),
        "import_statement" | "import_from_statement" | "use_declaration"
    ) {
        if let Ok(text) = node.utf8_text(source.as_bytes()) {
            imports.push((path.to_string(), text.to_string()));
        }
    }
    for index in 0..node.child_count() {
        if let Some(child) = node.child(index) {
            collect_imports(child, source, path, imports);
        }
    }
}

fn resolve_import_path(
    source_file: &str,
    import_text: &str,
    files: &HashMap<String, Vec<String>>,
) -> Option<String> {
    let source = Path::new(source_file);
    let mut candidates = Vec::new();
    if let Some(quoted) = import_text.split(['\'', '"']).nth(1) {
        if quoted.starts_with('.') {
            candidates.extend(module_candidates(&source.parent()?.join(quoted)));
        } else if !import_text.starts_with("use ") {
            candidates.push(format!("{}.py", quoted.replace('.', "/")));
            candidates.push(format!("{}/__init__.py", quoted.replace('.', "/")));
        }
    } else if let Some(path) = import_text.strip_prefix("use ") {
        let module = path.split("::").next().unwrap_or(path).trim();
        if !module.is_empty() && !matches!(module, "crate" | "self" | "super") {
            candidates.push(format!("{}.rs", module.replace("::", "/")));
            candidates.push(format!("{}/mod.rs", module.replace("::", "/")));
        }
    }
    candidates
        .into_iter()
        .find(|candidate| files.contains_key(candidate))
}

fn module_candidates(base: &Path) -> Vec<String> {
    let base = base.to_string_lossy().to_string();
    let mut candidates = vec![base.clone()];
    if Path::new(&base).extension().is_none() {
        candidates.extend([
            format!("{base}.js"),
            format!("{base}.jsx"),
            format!("{base}.ts"),
            format!("{base}.tsx"),
            format!("{base}/index.js"),
            format!("{base}/index.ts"),
        ]);
    }
    candidates
}

fn collect_declarations(
    node: tree_sitter::Node,
    source: &str,
    parser: &dyn SymbolParser,
    path: &str,
    out: &mut HashMap<String, Vec<String>>,
    symbols: &mut Vec<TopologySymbol>,
) {
    if parser.symbol_node_types().contains(&node.kind()) {
        if let Some(raw) = parser.extract_symbol_id(node, source) {
            let qualified = format!("{path}::{raw}");
            out.entry(raw).or_default().push(qualified.clone());
            let ast_node_type = match parser.node_category(node) {
                crate::ast::symbol_parser::AstNodeCategory::ApiDefinition => "API_DEFINITION",
                crate::ast::symbol_parser::AstNodeCategory::Implementation => "IMPLEMENTATION",
                crate::ast::symbol_parser::AstNodeCategory::Unknown => "UNKNOWN",
            };
            symbols.push(TopologySymbol {
                symbol_id: qualified,
                file_path: path.to_string(),
                ast_node_type: ast_node_type.to_string(),
            });
        }
    }
    for index in 0..node.child_count() {
        if let Some(child) = node.child(index) {
            collect_declarations(child, source, parser, path, out, symbols);
        }
    }
}

fn collect_calls(
    node: tree_sitter::Node,
    source: &str,
    parser: &dyn SymbolParser,
    declarations: &HashMap<String, Vec<String>>,
    calls: &mut Vec<(String, String)>,
) {
    if node.kind() == "call_expression" {
        if let Some(function) = node
            .child_by_field_name("function")
            .or_else(|| node.named_child(0))
        {
            if let Ok(text) = function.utf8_text(source.as_bytes()) {
                let name = text
                    .rsplit('.')
                    .next()
                    .unwrap_or(text)
                    .rsplit("::")
                    .next()
                    .unwrap_or(text);
                if let Some(target) = declarations
                    .iter()
                    .filter(|(raw, ids)| raw.ends_with(&format!(":{name}")) && ids.len() == 1)
                    .map(|(_, ids)| ids[0].clone())
                    .next()
                {
                    let mut owner = node.parent();
                    while let Some(candidate) = owner {
                        if parser.symbol_node_types().contains(&candidate.kind()) {
                            if let Some(raw) = parser.extract_symbol_id(candidate, source) {
                                if let Some(source_ids) = declarations.get(&raw) {
                                    if source_ids.len() == 1 {
                                        calls.push((source_ids[0].clone(), target));
                                    }
                                }
                            }
                            break;
                        }
                        owner = candidate.parent();
                    }
                }
            }
        }
    }
    for index in 0..node.child_count() {
        if let Some(child) = node.child(index) {
            collect_calls(child, source, parser, declarations, calls);
        }
    }
}
