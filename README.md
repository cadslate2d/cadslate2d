# CadSlate2D

**Classic 2D CAD. In your browser.**

CadSlate2D is a free, browser-native, local-first 2D CAD application inspired by classic command-driven drafting: a command line, keyboard aliases, precise coordinate entry, object snaps, grips, and local files. No installation, no account, no cloud workspace.

![CadSlate2D visual identity](visual_identity.png)

> The image above is the visual target, not a screenshot of the current build.

## Status

Milestone **M0 — The Line** is in progress: a vertical slice that proves the architecture with a single entity type. See the [milestones](https://github.com/LAA-Software-Engineering/cadslate2d/milestones) and [issues](https://github.com/LAA-Software-Engineering/cadslate2d/issues) for the plan in implementation order.

## Architecture in one paragraph

Rust compiled to WebAssembly owns all CAD state — entities, commands, selection, undo history, snapping, DXF — and prepares batched render data. TypeScript owns the React shell and the WebGL2 context and submits those batches to the GPU. The boundary between them is deliberately coarse: browser events go in, dirty flags come out, and frames are pulled in bulk. There is no backend; the production build is a folder of static files.

Details: [`docs/DESIGN.md`](docs/DESIGN.md) (implementation baseline) and [`docs/adr/`](docs/adr/README.md) (architecture decisions).

## Development

The workspace scaffold, build commands, and CI are being added under [epic M0-A](https://github.com/LAA-Software-Engineering/cadslate2d/issues/1). This section is updated as they land.

## Contributing

Read [`AGENTS.md`](AGENTS.md) first. It holds the architecture rules, engineering principles, testing rules, and workflow that apply to every change, whether written by a person or an AI agent.

## License

Not yet decided. CadSlate2D is free to use; open-source licensing is a separate governance decision (DESIGN §44).
