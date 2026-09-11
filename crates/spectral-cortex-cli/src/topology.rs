use anyhow::{Context, Result};
use git2::{Repository, TreeWalkMode, TreeWalkResult};
use spectral_cortex::model::smg_note::{TopologyLink, TopologyRelation};
use spectral_cortex::SpectralMemoryGraph;
use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::ast::registry::ParserRegistry;

/// Resolve deterministic local call edges from the repository `HEAD` tree.
pub(crate) fn resolve_call_links(
    repo_path: &Path,
    graph: &mut SpectralMemoryGraph,
    registry: &ParserRegistry,
) -> Result<usize> {
    let repo = Repository::open(repo_path).with_context(|| "opening repository for topology")?;
    let tree = repo
        .head()
        .and_then(|head| head.peel_to_commit())
        .and_then(|commit| commit.tree())
        .with_context(|| "reading repository HEAD tree for topology")?;
    let latest = latest_symbols(graph);
    let mut links = Vec::new();
    tree.walk(TreeWalkMode::PreOrder, |root, entry| {
        let Some(path) = entry.name() else {
            return TreeWalkResult::Ok;
        };
        let relative = format!("{}{}", root, path);
        let Some(ext) = Path::new(path).extension().and_then(|e| e.to_str()) else {
            return TreeWalkResult::Ok;
        };
        let Some(parser) = registry.get(ext) else {
            return TreeWalkResult::Ok;
        };
        let Some(object) = entry.to_object(&repo).ok() else {
            return TreeWalkResult::Ok;
        };
        let Some(blob) = object.as_blob() else {
            return TreeWalkResult::Ok;
        };
        let source = String::from_utf8_lossy(blob.content());
        let mut ts = tree_sitter::Parser::new();
        let language = match ext {
            "rs" => tree_sitter_rust::LANGUAGE,
            "py" => tree_sitter_python::LANGUAGE,
            "js" | "jsx" => tree_sitter_javascript::LANGUAGE,
            "ts" | "tsx" => tree_sitter_typescript::LANGUAGE_TYPESCRIPT,
            _ => return TreeWalkResult::Ok,
        };
        if ts.set_language(&language.into()).is_err() {
            return TreeWalkResult::Ok;
        }
        let Some(ast) = ts.parse(source.as_bytes(), None) else {
            return TreeWalkResult::Ok;
        };
        let mut declarations = HashMap::new();
        collect_declarations(
            ast.root_node(),
            &source,
            parser,
            &relative,
            &mut declarations,
        );
        collect_calls(
            ast.root_node(),
            &source,
            parser,
            &declarations,
            &latest,
            &mut links,
        );
        TreeWalkResult::Ok
    })?;

    let mut seen = HashSet::new();
    for (source, target) in links {
        if source == target || !seen.insert((source, target)) {
            continue;
        }
        if let Some(note) = graph.notes.get_mut(&source) {
            note.topology_links.push(TopologyLink {
                target_note_id: target,
                relation: TopologyRelation::Call,
            });
        }
    }
    Ok(seen.len())
}

fn latest_symbols(graph: &SpectralMemoryGraph) -> HashMap<String, u32> {
    graph
        .notes
        .values()
        .filter_map(|note| note.symbol_id.as_ref().map(|id| (id, note)))
        .fold(HashMap::new(), |mut out, (id, note)| {
            let replace = out
                .get(id)
                .and_then(|id| graph.notes.get(id))
                .map(|old| note.source_timestamps.iter().max() > old.source_timestamps.iter().max())
                .unwrap_or(true);
            if replace {
                out.insert(id.clone(), note.note_id);
            }
            out
        })
}

fn collect_declarations(
    node: tree_sitter::Node,
    source: &str,
    parser: &dyn crate::ast::symbol_parser::SymbolParser,
    path: &str,
    out: &mut HashMap<String, String>,
) {
    if parser.symbol_node_types().contains(&node.kind()) {
        if let Some(raw) = parser.extract_symbol_id(node, source) {
            out.insert(raw.clone(), format!("{path}::{raw}"));
        }
    }
    for index in 0..node.child_count() {
        if let Some(child) = node.child(index) {
            collect_declarations(child, source, parser, path, out);
        }
    }
}

fn collect_calls(
    node: tree_sitter::Node,
    source: &str,
    parser: &dyn crate::ast::symbol_parser::SymbolParser,
    declarations: &HashMap<String, String>,
    latest: &HashMap<String, u32>,
    links: &mut Vec<(u32, u32)>,
) {
    if node.kind() == "call_expression" {
        let Some(function) = node
            .child_by_field_name("function")
            .or_else(|| node.named_child(0))
        else {
            return;
        };
        let Ok(name) = function.utf8_text(source.as_bytes()) else {
            return;
        };
        let name = name
            .rsplit('.')
            .next()
            .unwrap_or(name)
            .rsplit("::")
            .next()
            .unwrap_or(name);
        let target = declarations
            .iter()
            .find(|(raw, _)| raw.ends_with(&format!(":{name}")))
            .map(|(_, qualified)| qualified);
        let Some(target) = target.and_then(|id| latest.get(id)) else {
            return;
        };
        let mut owner = node.parent();
        let mut source_id = None;
        while let Some(candidate) = owner {
            if parser.symbol_node_types().contains(&candidate.kind()) {
                if let Some(raw) = parser.extract_symbol_id(candidate, source) {
                    source_id = declarations
                        .get(&raw)
                        .and_then(|id| latest.get(id))
                        .copied();
                }
                break;
            }
            owner = candidate.parent();
        }
        let Some(source_id) = source_id else {
            return;
        };
        links.push((source_id, *target));
    }
    for index in 0..node.child_count() {
        if let Some(child) = node.child(index) {
            collect_calls(child, source, parser, declarations, latest, links);
        }
    }
}
