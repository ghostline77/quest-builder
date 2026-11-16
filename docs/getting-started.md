# Validate and build a quest

This walkthrough compiles the included time-gate example. It waits one game hour
and sets a completion fact, with no external resource bindings.

Run the commands from the Quest Builder repository root in PowerShell, after
[building the executable](../README.md#build).

## Set the executable path

```powershell
$qb = './target/release/ghostline-quest-builder.exe'
```

On Linux, use `./target/release/ghostline-quest-builder`.

## Validate and inspect the plan

```powershell
& $qb validate 'examples/blocks/time_gate.quest.json'
& $qb plan 'examples/blocks/time_gate.quest.json'
```

Validation checks the manifest's schema, routing, fact dataflow, block semantics
and ownership. Compilation additionally checks generated resource bindings.
The plan describes the quest structure without writing game resources.

## Generate resources

### Native build

Use `compile` to generate native binaries and resource documents:

```powershell
& $qb compile 'examples/blocks/time_gate.quest.json' --out 'work/time-gate-native'
```

The output directory must be new. Native serialization uses bundled reflection,
so this self-contained example needs neither a game directory nor a schema path.

For builds that resolve installed resources or generate a native bootstrap, add
`--game 'C:/Games/Cyberpunk 2077'` with your installation path. See
[compile options](reference.md#compile-options).

### Documents-only build

Use `--documents-only` to generate inspection documents without native binaries:

```powershell
& $qb compile 'examples/blocks/time_gate.quest.json' --documents-only --out 'work/time-gate-documents'
```

The output directory must be new. This output cannot be packaged for installation.

## Inspect the output

For the native build above:

```powershell
& $qb verify-build 'work/time-gate-native'
& $qb inspect 'work/time-gate-native/source/archive/mod/example_time_gate/phases/example_time_gate.questphase'
```

| Path inside the build | Contents |
| --- | --- |
| `source/raw/` | Generated CR2W-JSON documents, with `.json` appended to depot paths. |
| `source/archive/` | Native binaries. Absent in a documents-only build. |
| `source/resources/` | Registrations and supplied loose resources. |
| `build.json` | Resource hashes, dependencies and build status. |

Compilation does not install or activate the quest. Select a
[startup arrangement](reference.md#quest-startup), then follow
[packaging](reference.md#packages) and [installation](reference.md#publication-and-installation).

## Author a larger quest

Choose stage kinds from the [block examples](building-blocks.md). Include
the required scenes, world placements and published assets through the manifest's
[resource inputs](reference.md#authoring-inputs).

The CLI's `schema` command exports the **quest authoring JSON schema**.
It is separate from ghostline-red's bundled **resource reflection metadata**.
Use `catalog` to look up stage fields and ports:

```powershell
& $qb schema --out 'work/quest-authoring-schema.json'
& $qb catalog --markdown --out 'work/block-reference.md'
```
