# Spectral Cortex

## Overview

Spectral Cortex is an open-source Rust system for turning Git history into a structured memory layer for AI agents. It ingests commits, embeds semantically meaningful content, builds a spectral graph of related changes, and exposes retrieval interfaces through a CLI and MCP tools.

The project is designed for teams that want explainable, local-first memory over repository evolution: what changed, why it changed, and what other changes are semantically connected.

## One-Sentence Description

Spectral Cortex converts Git history into a spectral memory graph so AI agents can retrieve relevant project context with better structure, traceability, and temporal awareness.

## Short Description (For Websites)

Spectral Cortex is a Rust-based memory engine for AI development workflows. It ingests Git commits, builds a graph of semantic relationships, and supports retrieval through CLI and MCP integration.

## Long Description (For Press and Blogs)

Spectral Cortex introduces a practical way to use software history as machine-readable memory. Instead of treating commit logs as unstructured text, it transforms repository history into a Spectral Memory Graph (SMG), where each note represents one or more meaningful changes and each edge carries similarity strength.  

The system supports noisy and inconsistent commit messages, including mixed-format messages that contain multiple changes in a single commit. It can split those messages into multiple semantic notes to improve graph coherence and retrieval quality.  

For AI agent workflows, Spectral Cortex provides local querying with score transparency, temporal re-ranking, and compact MCP outputs. Teams can inspect why a result was returned, trace it back to source commits, and reason over connected decisions across time.

## Problem It Solves

Software teams accumulate critical context in Git history, but that context is hard for people and agents to retrieve quickly and reliably.

Common pain points:

1. Commit messages often mix unrelated changes.
2. Important context is buried in long histories.
3. Keyword search misses semantic relationships.
4. AI assistants lack durable, project-specific memory.

Spectral Cortex addresses this by building a structured, queryable graph from repository history and exposing it through agent-friendly interfaces.

## Core Capabilities

1. Git ingestion with configurable filtering for noisy lines.
2. Commit message segmentation (`off`, `auto`, `strict`) for multi-change commits.
3. Embedding-backed semantic indexing.
4. Spectral graph construction with tunable build parameters.
5. Related-note links with similarity scores.
6. Long-range links for topological but semantically distant connections.
7. Temporal re-ranking to favor fresh context when needed.
8. Query and note inspection via CLI (`ingest`, `update`, `query`, `note`).
9. Built-in MCP server mode via `spectral-cortex mcp --smg <path>`.

## Key Differentiators

### 1) Graph-Centric Memory, Not Flat Search
Spectral Cortex uses graph relationships and spectral structure, not only nearest-neighbor matching.

### 2) Commit-Aware Segmentation
It handles real-world commit message quality by splitting multi-topic commits into separate notes when confident.

### 3) Scored Relationships in Storage
Per-note related links store similarity values, enabling score-based filtering and visualization.

### 4) Local-First and Explainable
Outputs include note IDs, source commits, timestamps, and scoring metadata for grounded reasoning.

### 5) Agent Integration Through MCP
The same binary can run an MCP server with markdown-first, token-efficient tools.

## Architecture Summary

At a high level:

1. Collect commits from a target repository.
2. Apply optional line-level filtering.
3. Split commit messages into segments (based on configured mode).
4. Convert segments to embeddings.
5. Build spectral structures and cluster organization.
6. Compute related-note links and long-range links.
7. Persist as versioned SMG JSON.
8. Retrieve through CLI or MCP tools.

### Main Data Concepts

1. `Note`: normalized semantic unit in the graph.
2. `related_note_links`: adjacency list with `(note_id, spectral_similarity)`.
3. `long_range_links`: cross-graph links `(note_a, note_b, spectral_similarity)`.
4. Source provenance: turn IDs, commit IDs, timestamps.

## MCP Integration

Spectral Cortex includes MCP serving in the primary CLI:

```bash
spectral-cortex mcp --smg smg.json
```

MCP tools:

1. `graph_summary`
2. `query_graph`
3. `inspect_note`
4. `long_range_links`

The graph is preloaded once at startup, and tool inputs no longer require per-request `smg_path`.

## Typical Workflow

### Initial Build

1. Ingest repository history into `smg.json`.
2. Spectral structures are built as part of ingest/update.

### Incremental Maintenance

1. Run `update` with append+incremental behavior.
2. Persist updated graph.

### Agent Runtime

1. Load graph for retrieval via CLI or MCP.
2. Query for relevant notes.
3. Use commit provenance and related links as evidence in agent prompts.

## Who It Is For

1. AI engineering teams building code-aware assistants.
2. Platform teams managing large monorepos.
3. Developer tooling teams building retrieval and memory workflows.
4. Organizations requiring local, auditable context retrieval.

## Example Use Cases

1. Explain why a subsystem was refactored.
2. Trace recurring failure patterns across commits.
3. Find semantically related fixes when investigating regressions.
4. Build commit-grounded context packs for code review assistants.
5. Surface long-range architectural relationships missed by lexical search.

## Product Positioning

### Category
Agent memory infrastructure for software repositories.

### Positioning Statement
For teams building AI-assisted software workflows, Spectral Cortex is a Git-native memory graph engine that provides structured, explainable retrieval over code history, unlike flat text search or opaque memory systems.

## Messaging Framework

### Primary Message
Turn repository history into usable memory for AI agents.

### Supporting Messages

1. Better retrieval through graph structure and spectral relationships.
2. Strong provenance: every result ties back to commits and timestamps.
3. Built for real Git data, including noisy and mixed commit formats.
4. Integrated MCP mode for immediate agent interoperability.

## Copy Blocks for Media and Marketing

### Press-Style Blurb
Spectral Cortex is an open-source Rust project that transforms Git history into a spectral memory graph for AI agents. It supports semantic retrieval, scored graph links, temporal re-ranking, and MCP-native workflows, enabling teams to ground AI behavior in real repository context.

### Website Hero Copy
**Git History, as Agent Memory**  
Spectral Cortex converts commits into a spectral graph your AI tools can query, inspect, and trust.

### Website Subheadline
Ingest. Segment. Link. Retrieve.  
One binary for CLI workflows and MCP server integration.

### Social Post (Short)
Spectral Cortex turns Git history into a spectral memory graph for AI agents: semantic retrieval, scored related links, temporal ranking, and MCP integration in one Rust CLI.

## Technical Fact Sheet

1. Language: Rust
2. Storage format: versioned JSON SMG
3. Retrieval modes: semantic + temporal re-ranking
4. Graph relationships: related links with similarity scores, long-range links
5. Interfaces: CLI and MCP (`mcp` subcommand)
6. Update model: full ingest and incremental update flows
7. Commit handling: configurable split modes for multi-topic messages

## Reliability and Operational Notes

1. Incremental update skips already-seen commits by commit ID.
2. Graph outputs include explicit scoring/provenance fields for inspectability.
3. Strict JSON format versioning is enforced for load compatibility.
4. On macOS installs, runtime Torch dylibs are provisioned for installed binary execution.

## Suggested Visuals for Presentations

1. Pipeline diagram: Git commits -> filters/splitter -> embeddings -> spectral graph -> query/MCP.
2. Before/after example of split commit messages becoming multiple notes.
3. Graph screenshot showing weighted edges and linked neighborhoods.
4. Retrieval result panel with scores, commit IDs, and timestamps.
5. MCP integration slide showing single-command server startup.

## Suggested Slide Outline

1. Problem: software history is underused memory.
2. Approach: spectral graph over commit semantics.
3. How it works: ingest -> split -> embed -> build -> retrieve.
4. Why it’s different: scored links + explainability + MCP.
5. Workflow: initial ingest, incremental update, runtime querying.
6. Example output and provenance trace.
7. Integration model and next steps.

## FAQ

### Is this only for commit messages?
No. The model is designed for commit-centric workflows today, but the graph format can represent other short project text sources.

### Why split commit messages?
Many commits contain multiple unrelated changes. Splitting improves semantic precision and graph quality.

### Can I run it locally?
Yes. Spectral Cortex is intended for local and self-managed environments.

### How do agents connect?
Use the built-in MCP mode:

```bash
spectral-cortex mcp --smg smg.json
```

### Does it support time-aware retrieval?
Yes. Temporal re-ranking can be configured or disabled per query.

## Boilerplate

Spectral Cortex is an open-source Rust project that builds spectral memory graphs from Git history to improve AI agent retrieval and reasoning over repository context. It provides commit-aware segmentation, scored semantic links, temporal re-ranking, and integrated MCP tooling.

## Citation Guidance (Internal/External Content Teams)

When writing external content:

1. Prefer concrete, verifiable claims tied to observable CLI behavior.
2. Avoid unverified performance numbers unless benchmark methodology is published.
3. Highlight provenance and explainability features as primary trust signals.
4. Use consistent terminology: “Spectral Memory Graph (SMG)”, “related_note_links”, “long_range_links”, and “temporal re-ranking”.
