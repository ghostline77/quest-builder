# Quest Builder

Create Cyberpunk 2077 quests, scenes, journals and world placements from JSON
authoring specifications. The compiler generates resources directly through
`ghostline-red`.

| Task | Documentation |
| --- | --- |
| Validate and build a quest | [Getting started](docs/getting-started.md) |
| Choose a stage kind | [Building blocks and examples](docs/building-blocks.md) |
| Author dialogue, actors and scene connections | [Scene authoring](docs/scene-authoring.md) |
| Place lootable containers | [Loot containers](docs/loot-containers.md) |
| Include scenes, world data, characters or media | [Authoring inputs](docs/reference.md#authoring-inputs) |
| Choose how a quest starts | [Quest startup](docs/reference.md#quest-startup) |
| Package and install a build | [Packages](docs/reference.md#packages) and [installation](docs/reference.md#publication-and-installation) |
| Inspect resources and record playtest evidence | [Inspection and evidence](docs/reference.md#inspection-scenarios-and-evidence) |

## Build

Build from this repository with a stable Rust toolchain and Node.js 20 or newer.
Cargo fetches `ghostline-red` from GitHub. The frontend build installs its locked
`ui-core` GitHub dependency automatically; no sibling checkouts are required:

```powershell
cargo build --release --locked
```

Library consumers can set `default-features = false` to skip the editor frontend
and its Node.js build requirement.

The executable is `target/release/ghostline-quest-builder.exe` on Windows, or
`target/release/ghostline-quest-builder` on Linux.

Reflection metadata is bundled. No reflection-schema generation, schema path
or WolvenKit installation is required. Use `--game` when a build needs installed
game resources or native bootstrap generation.

## Open the editor

Pass one or more quest-project directories as workspaces. Each project has a
`project.json`; replace the example paths with your own project checkouts:

```powershell
cargo run --release --locked -- serve --workspace ../ghostline ../velvet-kade ../test-quests
```

Open **http://127.0.0.1:8790/**, then choose a project and quest.

| View | Tasks |
| --- | --- |
| Story | Edit stage titles and descriptions, arrange stages and connect outcomes. |
| Scenes | Write dialogue, cast actors and connect choices. |
| Locations | Import captured positions and edit world placements. |
| Cast | Review appearances, clothing, loadout and scene usage. Open Character Builder for visual changes. |
| Advanced | Access source JSON, graphs, all fields, project settings and compiler tools. |

Edits autosave. Undo, saved versions and disk-conflict comparison are available
in the workbench. Set the game directory in **Advanced → Project settings**.

The placement map and scene timeline display authored data. They do not simulate
terrain, collision or engine execution. See the [workbench reference](docs/reference.md#authoring-workbench).

For CLI commands, start with the [build walkthrough](docs/getting-started.md).
Use `ghostline-quest-builder <command> --help` for command-specific options.
