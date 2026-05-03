# Lexum-cli

Command-line interface for interacting with Lexum programs and runtime.

## Overview

`Lexum-cli` provides developer tooling for:

- compiling Lexum source files into executable bundles
- launching the Lexum runtime
- replaying deterministic execution journals
- inspecting convergence behaviour and runtime state

The CLI is intended to remain lightweight while exposing essential operational
controls for orchestration workflows.

## Planned Commands

- `Lexum compile`
- `Lexum run`
- `Lexum replay`
- `Lexum inspect`

## Design Goals

- Clear and predictable developer experience
- Minimal abstraction over runtime semantics
- Scriptable interface for automation environments
- Structured logging and diagnostic output

## Status

Early scaffolding stage.

Initial milestones include:

- argument parser
- runtime launcher integration
- basic execution tracing output

## License

Apache License 2.0
