# spectral-cortex

`spectral-cortex` is a command-line tool for indexing Git history as a Spectral
Memory Graph (SMG). It supports semantic search, repository status checks, and
an MCP server for AI agents.

## Install

Install a prebuilt release with `cargo-binstall`:

```sh
cargo binstall spectral-cortex
```

Or install from crates.io with Cargo:

```sh
cargo install spectral-cortex
```

## Quick start

Run these commands from a Git repository:

```sh
spectral-cortex init
spectral-cortex ingest
spectral-cortex query --query "why did we add this feature?"
```

Start the MCP server over standard input and output with:

```sh
spectral-cortex mcp
```

See the [project README](https://github.com/mrorigo/spectral-cortex) for the
full CLI reference, configuration options, and MCP client setup.
