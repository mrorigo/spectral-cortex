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
        let mut declarations = HashMap::new();
        collect_declarations(
            ast.root_node(),
            &source,
            parser,
            &path,
            &mut declarations,
            &mut symbols,
        );
        collect_calls(ast.root_node(), &source, parser, &declarations, &mut calls);
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

fn collect_declarations(
    node: tree_sitter::Node,
    source: &str,
    parser: &dyn SymbolParser,
    path: &str,
    out: &mut HashMap<String, String>,
    symbols: &mut Vec<TopologySymbol>,
) {
    if parser.symbol_node_types().contains(&node.kind()) {
        if let Some(raw) = parser.extract_symbol_id(node, source) {
            let qualified = format!("{path}::{raw}");
            out.insert(raw, qualified.clone());
            symbols.push(TopologySymbol {
                symbol_id: qualified,
                file_path: path.to_string(),
                ast_node_type: format!("{:?}", parser.node_category(node)),
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
    declarations: &HashMap<String, String>,
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
                    .find(|(raw, _)| raw.ends_with(&format!(":{name}")))
                    .map(|(_, id)| id.clone())
                {
                    let mut owner = node.parent();
                    while let Some(candidate) = owner {
                        if parser.symbol_node_types().contains(&candidate.kind()) {
                            if let Some(raw) = parser.extract_symbol_id(candidate, source) {
                                if let Some(source_id) = declarations.get(&raw) {
                                    calls.push((source_id.clone(), target));
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
