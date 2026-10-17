# Muxin, as this tree is now

Muxin 0.5.6. One program on this computer, the other computers you own, and GitHub through your `gh` login. This folder is a snapshot of that design. The [README](../README.md) is how you install it and the command cookbook. [AGENTS.md](../AGENTS.md) is how an agent should behave while using it.

The diagram uses the shipped diagram-design skin (paper `#f5f5f5`, ink `#2d3142`, accent `#bf4520`). It is not the app's theme.

Researched from `Cargo.toml`, `src/main.rs`, `src/api.rs`, `src/mcp.rs`, `src/ui/root.rs`, `src/fleet/`, `src/cloud/`, `src/ops/`, `agent/src/main.rs`, and the README. Public surface: 47 top-level CLI commands, 89 MCP tools, 19 window routes. Concepts: one API with three doors, ssh then a live link or QUIC, sessions that outlive the window, DigitalOcean as the only cloud. Design decisions called out below: no second cloud provider, no daemon on this computer, sessions live in `muxen-agent`.

```
Documentation plan:
  App shape              tutorial  how-to  reference  explanation
  One API, three doors   no        no      surfaces   explanation-architecture
  Reaching a machine     no        no      surfaces   explanation-connections
  What needs attention   no        howto   surfaces   no
  Cloud (DigitalOcean)   no        no      reference-cloud  no
```

There is no tutorial here. The README already walks install to `muxin add-machine`, and a second tutorial would drift from it.

## Diagram

[How Muxin sits between you and a machine](diagrams/architecture.html). Architecture, five nodes, doc-inline. Open it in a browser.

## Explanation

- [Why Muxin is one program with three doors](explanation-architecture.md)
- [How Muxin reaches a machine](explanation-connections.md)

## How-to

- [How to see what needs attention](howto-see-what-needs-attention.md)

## Reference

- [Surfaces: window, CLI, MCP](reference-surfaces.md)
- [Cloud machines](reference-cloud.md)

## What this snapshot leaves out

Runner install, desktop input, and `cloud create` change a real machine or start a bill. The how-to stops at reading. The reference names the commands and says which ones wait for you to ask.

Cloudflare Workers and R2 are not in the tree. Cloud is DigitalOcean only.
