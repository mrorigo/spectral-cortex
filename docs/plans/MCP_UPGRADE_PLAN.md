# MCP Upgrade Plan

## Objective

Upgrade Spectral Cortex's MCP server from the `rmcp 0.15` / pre-2026 protocol
implementation to the MCP `2026-07-28` revision, while preserving the existing
local stdio workflow and compatibility with clients that still use the
`2025-11-25` lifecycle.

The first release is a protocol/runtime upgrade, not an expansion of the
product surface. The existing six graph tools remain the primary interface.
Resources, prompts, remote HTTP transport, Tasks, elicitation, and sampling are
explicitly deferred unless a later requirement makes one necessary.

## Definitions

- **MCP:** Model Context Protocol.
- **RMCP:** the official Rust MCP SDK and its `rmcp` crate.
- **SMG:** Spectral Memory Graph, the graph loaded by this server.
- **Protocol era:** the request lifecycle associated with a protocol revision.
- **Modern era:** MCP `2026-07-28`, which uses per-request metadata and no
  protocol-managed session.
- **Legacy era:** MCP `2025-11-25` or an earlier revision that uses the
  initialize/initialized handshake.

## Current-state facts

1. The CLI depends on `rmcp = 3.2.0` with `server`, `macros`, and
   `transport-io` features.
2. `crates/spectral-cortex-cli/src/mcp_server.rs` uses `ServerHandler`,
   `ToolRouter`, `#[rmcp::tool]`, and `.serve(stdio())`.
3. The server advertises tools only.
4. It exposes six tools: `query_graph`, `inspect_note`, `long_range_links`,
   `graph_summary`, `get_structural_hotspots`, and `inspect_symbol_history`.
5. Tool handlers return `CallToolResult`; successes contain markdown plus a
   `{ "markdown": string }` structured result, and failures return RMCP
   `ErrorData`.
6. The SMG is loaded once at startup and held in an `Arc`; tool calls do not
   mutate it.
7. The repository has catalog and result-shape unit tests, but no subprocess
   stdio protocol integration tests yet.

These facts are grounded in the current manifest and
`crates/spectral-cortex-cli/src/mcp_server.rs`; they are not assumptions about
the intended future architecture.

## Target protocol policy

### Supported protocol eras

The server will support both:

| Era | Policy |
|---|---|
| MCP `2026-07-28` | Supported and preferred. Use modern discovery/stateless metadata when the selected RMCP API exposes it. |
| MCP `2025-11-25` and earlier supported RMCP versions | Retain compatibility during the migration unless the SDK makes dual-era support impossible without unsafe behavior. |

Do not claim modern-only support until a wire-level test proves that a
`2026-07-28` client can discover and call all six tools.

### Transport policy

1. Keep stdio as the only production transport in this upgrade.
2. Do not add Streamable HTTP, OAuth, proxy routing headers, or session stores
   without a separate requirement.
3. Ensure all diagnostics go to stderr; stdout remains MCP JSON-RPC traffic.
4. Do not add protocol session state. The loaded graph may remain application
   state because it is immutable during the process lifetime.

### Result policy

Each tool will return:

1. A human-readable markdown text content block, preserving the current output
   style.
2. Structured JSON content with a stable output type where practical.
3. A protocol-level tool error for invalid input or execution failure, rather
   than an `Ok` response containing `Error: ...`.

The structured result is additive for clients that consume machine-readable
content. Existing markdown consumers must continue to receive equivalent
information.

## Scope and non-goals

### In scope

- Upgrade RMCP to the current 3.x release that supports MCP `2026-07-28`.
- Migrate compile-time API changes in the server and tool macros.
- Advertise the server identity and supported capabilities correctly.
- Support modern discovery/lifecycle behavior and legacy compatibility.
- Make tool errors distinguishable from successful results.
- Add output schemas and structured result types for the six tools.
- Add stdio protocol/conformance tests.
- Add bounded input validation and explicit limits to every tool.
- Update MCP documentation and client configuration examples.

### Out of scope

- Adding write/mutating tools.
- Resources or resource subscriptions.
- Prompts.
- Sampling or elicitation.
- Tasks and long-running request handles.
- Remote HTTP deployment or OAuth authorization.
- Replacing the SMG query implementation.

## Deterministic requirements

### R1 — Dependency and lifecycle

1. `rmcp` must be pinned to the selected stable 3.x release in the manifest
   and lockfile. The current implementation selects `3.2.0`.
2. The server must compile with the repository's normal feature set.
3. A modern client must be able to discover the server and call a tool using
   MCP `2026-07-28`.
4. A legacy-compatible client must still be able to initialize and call a tool,
   unless a documented SDK limitation makes this impossible.
5. The server must not require `Mcp-Session-Id` for modern requests. RMCP
   3.2.0 remains responsible for protocol negotiation and lifecycle behavior.

### R2 — Tool catalog

`tools/list` must expose exactly the six existing tool names, with:

- non-empty descriptions;
- an object-root input schema;
- declared output schemas;
- deterministic ordering;
- explicit bounds for numeric limits.

### R3 — Tool results and errors

1. Successful calls return markdown plus structured content.
2. Unknown note IDs, invalid limits, invalid score thresholds, and graph query
   failures produce protocol-visible errors.
3. No successful tool response may use the literal `Error:` convention to
   represent failure.
4. Existing result fields and semantics remain backward compatible unless the
   change is documented.

### R4 — Process behavior

1. Startup failures return a non-zero process result and a diagnostic on stderr.
2. Malformed JSON-RPC input does not cause an uncontrolled panic.
3. SIGINT/EOF causes graceful shutdown.
4. No application diagnostic is written to stdout.

### R5 — Security and resource bounds

1. The stdio server reads only the explicitly supplied SMG path.
2. Tool inputs cannot select arbitrary filesystem paths or execute commands.
3. `top_k`, link counts, snippet sizes, and history limits have documented hard
   upper bounds.
4. Tool descriptions do not claim capabilities the server does not provide.

## Implementation phases

### Phase 0 — Establish a migration branch and baseline

Record the current behavior before changing the SDK:

1. Capture `cargo tree -p spectral-cortex --edges normal` and the exact RMCP
   version selected in `Cargo.lock`.
2. Run the existing CLI/library tests.
3. Use MCP Inspector or an equivalent stdio client to record the current
   `tools/list` and one successful `tools/call` response.
4. Save representative responses for all six tools as compatibility fixtures.

### Phase 1 — Upgrade RMCP

1. Update the manifest and lockfile to the selected RMCP 3.x release.
2. Resolve macro, model, `ServerInfo`, lifecycle, and transport API changes.
3. Keep the stdio transport and avoid enabling unrelated transport features.
4. Explicitly configure supported protocol versions if the new SDK defaults do
   not match the dual-era policy.
5. Verify that the server advertises tools and does not accidentally advertise
   resources, prompts, or experimental extensions.

### Phase 2 — Migrate the server contract

1. Replace `String`-only tool result paths with RMCP result types.
2. Define typed structured result models for each tool.
3. Preserve markdown as a text content block.
4. Map domain failures to protocol errors with stable, client-visible
   messages.
5. Add output schemas generated from the typed result models.
6. Keep tool ordering deterministic.

### Phase 3 — Add validation and protocol tests

Build tests around a fixture SMG and an in-process or subprocess stdio client.
The suite must cover:

1. Modern discovery and protocol metadata.
2. Legacy initialize compatibility, if retained.
3. `tools/list` names, descriptions, schemas, and order.
4. One successful call for every tool.
5. Unknown tool, malformed arguments, invalid numeric bounds, missing note,
   empty graph, and query failure cases.
6. Structured content and markdown content in successful responses.
7. Protocol error responses for failed calls.
8. EOF and malformed-message handling.
9. stdout purity and graceful shutdown.

Tests must inspect wire responses, not only direct Rust method calls.

### Phase 4 — Documentation and compatibility release

1. Update `README.md`, `docs/USER-GUIDE.md`, and `docs/SPECTRAL_CORTEX.md` with
   the supported protocol policy and exact launch command.
2. Document that stdio is local-only and that the SMG path is supplied at
   process startup.
3. Document the six tools, structured result availability, and hard limits.
4. Document the minimum client behavior required for modern discovery and the
   fallback behavior for legacy clients.
5. Add an upgrade note stating that RMCP 3.x is a breaking internal dependency
   migration even though the CLI tool names remain stable.

## Decision table for compatibility

| Client behavior | Expected result |
|---|---|
| Modern discover, then `tools/list` | Discovery succeeds; six tools are returned in deterministic order. |
| Modern discover, then valid `tools/call` | Structured and markdown content are returned. |
| Modern `tools/call` with invalid arguments | Protocol-visible invalid-params/tool error; not a successful text error. |
| Legacy initialize, then `tools/list` | Works during the compatibility window. |
| Unknown tool name | Protocol error. |
| EOF on stdin | Graceful server termination. |
| Malformed JSON-RPC | Error response or controlled termination according to RMCP behavior; never an uncontrolled panic. |

## Verification gates

The upgrade is complete only when all gates pass:

1. `cargo fmt --all -- --check`.
2. `cargo check --workspace`.
3. `cargo clippy --workspace --all-targets -- -D warnings`, with unrelated
   pre-existing warnings either fixed or explicitly isolated in the change.
4. `cargo test --workspace`.
5. MCP wire-level tests for modern discovery and all six tools.
6. Legacy compatibility tests, or a documented compatibility decision backed by
   the SDK's limitation and an explicit release note.
7. Manual MCP Inspector validation of stdio startup, tool listing, one success,
   and one failure.

## Research basis

- [MCP specification 2026-07-28](https://modelcontextprotocol.io/specification/2026-07-28)
- [MCP 2026-07-28 release overview](https://blog.modelcontextprotocol.io/posts/2026-07-28/)
- [Official Rust SDK](https://github.com/modelcontextprotocol/rust-sdk)
- [RMCP 3.x migration history](https://github.com/modelcontextprotocol/rust-sdk/blob/main/rmcp/CHANGELOG.md)

The 2026-07-28 revision removes the initialize/session lifecycle for the
modern protocol era, introduces `server/discover`, moves server-to-client
interactions to Multi Round-Trip Requests, broadens structured tool content,
and formalizes extensions. Those features are why upgrading only the version
string would be insufficient.
