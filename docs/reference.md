# Quest Builder reference

Reference for authoring inputs, build outputs, packaging and inspection.
For a first CLI build, use [Validate and build a quest](getting-started.md).

## Authoring workbench

```powershell
cargo run --release --locked -- serve --workspace ../ghostline ../velvet-kade ../test-quests
```

Open `http://127.0.0.1:8790/` in a browser. Use `--port` to choose another port.
The server binds to loopback and only exposes the supplied workspace directories.
Release executables embed the browser assets; no frontend build or network access
is required.

The project browser groups quests, scenes, dialogue, characters, placement specs,
media and package profiles. Quest flow exposes every registered compiler block,
with contract-derived required fields and editable outcome routing. Scene graph,
actor, dialogue and timeline views edit the existing authoring declarations.
The Fields view provides recursive controls for every value, including fields
outside the specialised views. Source editing preserves 64-bit numeric IDs in
browsers supporting lossless JSON parsing.

Locations use a local top-down placement view with anchor-relative dragging,
zoom, exact coordinates and captured-position import. It displays authored
placements and trigger outlines; terrain, collision and final appearance need
in-game verification. The scene timeline displays authored timing rather than
simulating engine execution.

Edits autosave after a pause. Undo/redo applies to the open document; saved
versions can be restored from the inspector. External changes prevent overwrites
until the editor and disk versions have been compared or reloaded. Save history
and compiler job logs live under each workspace's ignored `work` directory.
Browser preferences stay in local browser storage.

**Advanced → Compiler tools** exposes CLI operations and options, including scene
and world inspection, scenarios, localization, packaging, installation, rollback
and evidence verification. Jobs run in the selected workspace, display their
output and can be cancelled. Reflection is bundled automatically. Supply the game
directory for installed resources; `--schema` is an optional advanced override.
Use `--documents-only` to generate inspection documents without native binaries.
Installation, rollback and build replacement show the operation before execution.

## Build and generate

```powershell
cargo build --release --locked

target/release/ghostline-quest-builder.exe schema --out work/quest-authoring-schema.json
target/release/ghostline-quest-builder.exe catalog --out work/blocks.json
target/release/ghostline-quest-builder.exe validate examples/signal.quest.json
target/release/ghostline-quest-builder.exe plan examples/signal.quest.json

target/release/ghostline-quest-builder.exe compile examples/signal.quest.json --inputs examples/signal.inputs.json --out work/signal-build --game "C:/Games/Cyberpunk 2077"
```

The `schema` command exports the quest authoring JSON schema, not resource
reflection metadata. Resource reflection is bundled automatically. `--game` adds the installed game's
reflected type information and allows inspection of game-owned called interfaces.
Published scene/phase binaries supplied through `assets` are inspected directly.
Passing `--documents-only` produces editable resource
documents without native binaries. Packaging requires a native build.

`--out` names a new directory. Resource generation and native readback finish
before the directory is published. A failed build leaves no partial output there.

The output contains:

- `source/raw`: generated resource documents, with `.json` appended to depot paths.
- `source/archive`: native resource binaries, unless `--documents-only` was selected.
- `source/resources`: ArchiveXL registrations and explicitly supplied loose outputs.
- `build.json`: output hashes, authoring fingerprint, generator identity, declared
  dependencies and native/prototype status.

`--allow-planned` permits prototype generation; those builds cannot be packaged.

### Compile options

| Argument | Type | Required | Behavior |
| --- | --- | --- | --- |
| `MANIFEST` | Path | Yes | Quest authoring JSON file. |
| `--out` | Path | Yes | New build directory; publication rejects an existing destination. |
| `--inputs` | Path | No | Resource input file for this build. |
| `--game` | Path | For installed-resource bindings, native bootstrap and character generation | Cyberpunk 2077 installation directory. |
| `--documents-only` | Flag | No | Skip native serialization. The result cannot be packaged. |
| `--allow-planned` | Flag | No | Generate a prototype including planned stages. Prototypes cannot be packaged. |
| `--schema` | Path | No | Advanced resource-reflection override. Unrelated to the authoring `schema` command. |
| `--character-builder` | Path | No | Override the executable used for requested series characters. |
| `--catalogue` | Path | No | Override the item catalogue used for character generation. |
| `--character-exports` | Path | No | Reuse existing character exports only when their saved projects match the authoring. |

## Authoring inputs

### Resource collections

See [building block examples](building-blocks.md) for a JSON quest manifest
for every supported stage kind, grouped by purpose, with binding requirements
and generation commands. Individual manifests live in [examples/blocks](../examples/blocks/).

Quest manifests use the canonical schema returned by `schema`. `expand` resolves
composition aliases and recipes without producing resources. Paths such as
`mod\signal\phases\signal.questphase` are generated depot identities.

The quest manifest's `resources` object collects other generated resources into
the same transaction. The CLI also accepts an explicit input file:

```json
{
  "worlds": ["signal.world.json"],
  "workspots": ["contact-rest.workspot-spec.json"],
  "scenes": [
    {"spec": "meeting.scene-spec.json", "dialogue": "meeting.dialogue.json"}
  ],
  "lifecycles": ["transfer.lifecycle.json"],
  "localizations": [
    {"spec": "meeting.localization.json", "manifest": "meeting.dialogue.json"}
  ],
  "assets": {
    "mod\\signal\\ui\\icon.xbm": {
      "path": "ui/icon.xbm"
    }
  },
  "loose": {
    "r6/tweaks/signal.yaml": {
      "path": "published/signal.yaml"
    }
  }
}
```

Paths are relative to the input file. Inline declaration objects are also
accepted. A scene specification may name its dialogue manifest through
`manifest`; that path is relative to the scene specification.

### Published assets

`extra_resources` imports explicitly generated resource documents from other
tools, such as the character or building builder. In the Rust API these are
in-memory documents; the CLI also accepts paths to those documents. They are not
used as bases for generating quest resources.

`assets` freezes already published binary outputs from other tools. `loose`
does the same for TweakXL, REDscript and other loose resources. Hash mismatches
fail generation. Supplied ArchiveXL files are parsed and merged into the generated
registration. A stage's `required_assets` becomes an explicit package dependency.
Paths and resource ownership are checked for collisions, including case aliases.

`archives` accepts published character or building archives with embedded depot
identities. Their payloads are frozen into the build receipt. Quest phases,
scenes, journals and lipsync maps are generated by this compiler; they cannot be
imported from a character/asset archive. `media` directories supply published
WEM, animation, texture and font files arranged by their exact depot paths.

### Workspot definitions

`workspots` generates native animation hubs and workspot trees from a `depot`,
`rig`, `animsets` and a recursive `root`. Entries are `sequence`, `pause`, `clip`
or weighted `random` lists with unique keys. Sequences may declare an
idle animation and repeat policy. Invalid durations, clip names, counts or
weights fail generation. Existing animation setup hashes can be preserved with
`animation_setup_hash`; no workspot file is used as a generation base.

### UI and effects

`atlases`, `font_families` and `websites` generate native UI resources from
authored layouts. `effects` generates timed particle/effect/spawner resources
and their declared ArchiveXL patch registrations from authored texture sequences.
All mod-owned resource references in generated documents must resolve to another
generated resource or an explicitly supplied asset before native publication.

### Computer page activation

Quest-owned website pages need an active runtime journal state for the browser
to resolve their addresses. Follow SQ021's Randy-computer flow: activate pages
with native journal nodes, then wait for the fact in the page's `factsToSet`.
An `owned_scope` setup action can express this as
`{"kind":"journal","path":"internet_sites/site/page","class_name":"gameJournalInternetPage","file_entry_index":1,"state":"Active"}`.
Activate the site container similarly with `gameJournalInternetSite` when
introducing a new site. Keep the page's `read_fact` and the quest's `wait_fact`
separate from activation; only the browser's successful page spawn sets the
visit fact. The journal's `activatedAtStart` flag does not replace this explicit
quest setup for an entry that is inactive in the loaded save.

### Lipsync bindings

Supplied lipsync animation sets can live at a mod-owned path such as
`mod\\signal\\localization\\en-us\\lipsync\\meeting\\actor.anims`. Scene
`lipsync_animset` references that exact path; the compiler generates its lipmap
and registration. Preserve animation clip RUIDs when reusing published audio.

An encounter's optional `authoring` declarations request checks against the
worlds and TweakXL supplied in this build. They verify the boss entry/record,
activation policy, world references and explicit cyberpsycho record requirements.
No paths are resolved through another quest series or its generated output.

### Dialogue localization

Dialogue localization specs contain `namespace`, `dialogue` and optional `locale`
(default `en-us`). The same dialogue manifest supplies `spoken_lines` with stable
uint64 `string_id`, `text` and published WEM `audio_path`. `male_audio_path`,
`female_text` and `male_text` are optional. Shared audio stays shared when no male
path is supplied. This generates subtitle entries, a subtitle map, a voiceover map
and locale registration. Supplied WEMs enter the normal package dependency closure.
No audio is synthesized or converted. Explicit `subtitle_depot`, `subtitle_map_depot`
and `voiceover_depot` can preserve existing resource identities.

### Scenes and world placement

Scenes support community, player and scene-owned spawned actors, dialogue,
choices, named entries/exits, quest commands, RID animation/cameras and actor
gaze. [Scene authoring](scene-authoring.md) uses named node connections,
actors and animation references; the compiler allocates their native IDs.

World specifications declare their own namespace, prefab root and origin.
Markers, triggers, entities, devices, communities, activity spots and splines
are generated directly. Their ArchiveXL registrations are collected by `compile`.

Access-point devices can declare `personal_link` with the selected entity's
`controller_component_id` and optional `interaction_record`. The builder generates
the controller/persistent-state override in the placed instance: forced personal
link, inactive hacking skill check and skipped netrunner minigame. It retains the
explicit base entity reference and generates the device registry, without cloning
an `.ent` resource. Inspect the selected entity to obtain its controller ID; the
binding is specific to that entity and game version.

Individual resources can also be generated separately:

```powershell
target/release/ghostline-quest-builder.exe world world.json --out work/world-build
target/release/ghostline-quest-builder.exe scene meeting.scene-spec.json --dialogue meeting.dialogue.json --out work/scene-build
target/release/ghostline-quest-builder.exe lifecycle examples/transfer.lifecycle.json --out work/transfer-build
target/release/ghostline-quest-builder.exe world-plan examples/signal.world.json
target/release/ghostline-quest-builder.exe world-hash '#signal/#signal_origin'
target/release/ghostline-quest-builder.exe scene-audit meeting.scene-spec.json --dialogue meeting.dialogue.json
target/release/ghostline-quest-builder.exe localization examples/meeting.localization.json --out work/text-build
```

### Lifecycle resources

Lifecycle kinds are `meeting`, `personal_link_attempt`, `personal_link_transfer`,
`root`, `finish_and_epilogue` and `reward_reconcile`. They cover protected/rearmed
meetings, disconnect/retry flows, phone-call boundaries, verified completion,
epilogues and once-only reward repair. Each uses explicit facts, resources and
world bindings. Unknown fields fail rather than being silently ignored.

### Generated runtime flow

Quest stages own their runtime behavior. Timed transfers, retries, meeting
re-entry, activity restoration, rewards and cleanup are compiler implementations.
The compiler generates any supporting child phases automatically. The quest's
`resources` section references scene and world authoring and published media or
series character names; compile generates and verifies the complete resource set.

### Series characters

Define characters once in the series configuration:

```json
{
  "schema_version": 1,
  "characters": {"contact": "characters/contact.character.json"},
  "quests": [{"id": "signal", "manifest": "quests/signal/quest.json"}]
}
```

Each quest requests `"characters": ["contact"]` in its `resources`. Both `series`
and `compile` invoke the standalone character builder. A series invocation exports
each required character once and reuses it across quests. Generated character
resources and TweakXL are included automatically by the packager; package profiles
need no character-specific file lists. Conflicting resources across builds fail.

The builder is found beside the executable, or on PATH. `--character-builder` overrides it; `--catalogue`
selects the item catalogue (by default, `GHOSTLINE_ITEMS_CATALOGUE` or the project’s `catalogue` directory). Character
generation requires `--game`; reflection is automatic. Planned quests validate their character
references without generating them until selected for compilation.

## Performance blocks

`performance_blocks` supports `workspot_move`, `workspot_idle`, `workspot_exit`
and `pose_beat`. Pose presets are procedural builders: `seated_sad_reply`,
`seated_explanation`, `arrive_and_sit`, `stand_from_chair`, and
`group_side_exchange`. Their animation sets require compatible actor rigs,
furniture and props.

Bind every preset role to a declared actor. Bind or explicitly ignore every
output port. Root-motion beats require a world origin, a complete offset
transform and a named `workspot` referencing a declared scene workspot. Scene
workspots declare their resource, origin and local transform; local IDs are
generated within the scene. `anchor_line` places a beat relative to a dialogue
line's actual start time.

## Quest startup

Compiling a root questphase does not activate it in the game. Its startup
registration or parent controller must be included in the installed package.

All three arrangements below generate native game resources. Native serialization
alone does not determine whether a package needs the PC ArchiveXL plugin or can
be used through the cross-platform loader.

| Arrangement | What starts the quest | Authoring and integration | Runtime requirement |
| --- | --- | --- | --- |
| PC mod using ArchiveXL | ArchiveXL attaches the root using generated `quest.phases` registration. | Leave `resources.bootstrap` unset or false. Package the generated registration with the quest. | PC ArchiveXL plugin. Quest Core is not needed. |
| Cross-platform mod.io quest using Quest Core | The shared core calls the mod's allocated slot wrapper, which calls its root. | Leave `resources.bootstrap` unset or false. A Rust integration calls `quest_core::attach` after compilation. | Quest Core installed separately. No PC ArchiveXL plugin is needed for this startup path. |
| Standalone native bootstrap | A conversation scene signals an active persistent quest host, which calls the root. | Set `resources.bootstrap` to true and compile with `--game`. | The package supplies its own bootstrap overrides. No PC ArchiveXL plugin is needed for this startup path. |

### PC mods using ArchiveXL

The default compiler output includes a registration entry under `quest.phases`
for its root. This is the normal PC ArchiveXL startup arrangement; there is no
need to enable the conversation bootstrap for a mod using that registration.

### Cross-platform mod.io quests using Quest Core

The current mod.io exporter rejects `quest` registration. Converting an ArchiveXL
registration file to JSON or moving it into `r6/c77` does not supply a replacement
startup mechanism.

Use `quest_core::attach` to generate the allocated slot wrapper and remove the
root's separate quest registration. The shared core owns the conversation area,
registered scene and persistent host overrides. Individual slot packs supply
their own wrapper and gameplay resources, without another global bootstrap.
Do not enable `resources.bootstrap` on those slot packs.

The cross-platform loader accepts supported JSON archive registration, such as
streaming, journal and localization entries. A JSON `.archive.xl` filename does
not itself mean that the package needs the PC ArchiveXL DLL. The registration
features and startup arrangement determine that dependency.

### Standalone native bootstrap

For a package that owns its own startup mechanism, set this field in the quest
manifest's `resources` object:

```json
{"resources": {"bootstrap": true}}
```

Compile with `--game`. The compiler generates the conversation request scene and
the vanilla overrides needed to launch the root from an active quest host. It
removes that root's separate `quest.phases` registration.

Independently installed packages using this bootstrap override the same vanilla
resources and can conflict. Use the shared Quest Core arrangement when quest
packs need independent slots. See [quest bootstrap](scene-authoring.md#quest-bootstrap)
for the generated resources and phase interface.

### Starting conditions

Set quest `start_gate` to `"after_act_1"` to delay all activation, including phone
offers, until **Playing for Time** is complete. Phantom Liberty standalone
starts also qualify. The gate runs before `start_condition`; omit it for quests
that should be available earlier.

## Packages

The `package` CLI command produces the normal PC package layout. Startup and
distribution are separate choices: a PC archive may use ArchiveXL registration
or the native bootstrap, depending on its build inputs.



World `sector_patches` can remove vanilla activity spots. Each entry contains
`path`, `expected_nodes`, and `node_deletions` (`index` and native `type`). Select
the same sector paths in the package profile's `streaming_sectors` list.

```powershell
target/release/ghostline-quest-builder.exe package examples/signal.package.json --plan
target/release/ghostline-quest-builder.exe package examples/signal.package.json --out work/signal-package
```

Profiles refer to generated build directories relative to the profile file.
They select active/support/additional roots, inclusion and exclusion prefixes,
loose files, streaming sector patches and optional external patch targets.
Active roots require exactly one ArchiveXL registration.

Dependency selection reads native imports, authored resource paths and resolvable
resource hashes. Missing owned dependencies fail the package. Game dependencies
are reported separately. Localized lip-sync redirects require matching language
map evidence. Unresolved numeric resource hashes are reported and rejected unless
the profile explicitly permits them.

Mesh deduplication creates deterministic ArchiveXL aliases;
`deduplicate_meshes: false` retains the original copies for comparison.
`minimum_archivexl` is recorded in the verification receipt. Build receipt hashes
are checked before packaging, freezing the selected bytes.

Package output includes an installable folder layout, a ZIP, `package.json` and
`verification.json`. The archive is read back and every entry is extracted and
compared with the frozen input. ZIP paths and extracted bytes are checked too.
Packaging does not install anything into the game.

### Cross-platform packages

Rust integrations use `package::publish_modio` for the local `r6/c77` package
layout. This is a separate export path; the `package` CLI has no `--modio` option.
Renaming or relocating a PC package is not a cross-platform export.

The exporter checks resource types, loose files and registration features. It
rejects `quest` registration and resource patches. A Quest Core slot pack or
standalone bootstrap must also satisfy those checks for all of its other content.
An ArchiveXL-free startup does not make every resource in the quest compatible.

Native readback verifies serialization and authored values. Archive/ZIP checks
verify distribution bytes. In-game behaviour needs separate playtest evidence;
generated receipts keep `runtime_tested` false.

## Publication and installation

```powershell
target/release/ghostline-quest-builder.exe verify-build work/signal-build
target/release/ghostline-quest-builder.exe replace-build work/signal-candidate work/signal-build
target/release/ghostline-quest-builder.exe install work/signal-package --game "C:/Games/Cyberpunk 2077"
target/release/ghostline-quest-builder.exe verify-installed signal --game "C:/Games/Cyberpunk 2077"
target/release/ghostline-quest-builder.exe rollback signal --game "C:/Games/Cyberpunk 2077"
```

Replacement verifies both builds, rejects changed/unowned files and retains the
previous directory as the reported `backup`. Pass that backup as the candidate to
restore it. A failed publication restores the previous output.

Installation is explicit. It verifies package bytes, records file ownership and
backs up overwritten files. Updates retire only files owned by that package.
Verification and rollback detect independent edits; rollback restores the previous
installation, including preexisting files overwritten by the first install.

`project <directory> [--depot <identity>]` resolves a standalone `project.json`
with `schema_version`, `id`, `depot_prefixes` and optional `dependencies`.
Dependencies are relative project directories. Cycles and duplicate owners fail;
resource ownership uses the longest matching declared prefix.

## Inspection, scenarios and evidence

```powershell
target/release/ghostline-quest-builder.exe graph examples/signal.quest.json --out work/signal-graph.md
target/release/ghostline-quest-builder.exe scenarios examples/signal.quest.json examples/signal.scenarios.json
target/release/ghostline-quest-builder.exe catalog --examples examples/signal.quest.json --markdown --out work/blocks.md
target/release/ghostline-quest-builder.exe inspect resource.questphase --query completion
target/release/ghostline-quest-builder.exe index published/resources --out work/corpus.json
target/release/ghostline-quest-builder.exe query work/corpus.json completion --kind questQuestPhaseResource
target/release/ghostline-quest-builder.exe analyze work/corpus.json --out work/analysis.json
```

Inspectors report graph endpoints, facts, scene/actor/animation details, appearances,
components, localization, journals and world resources. Typed occurrences and
events retain JSON pointers. Research indexes accept explicit resources,
directories and archives. Archive hashes without names are reported; supply a
JSON string array with `--known-paths` to resolve additional names.

`index --resume <previous-index>` reuses summaries only when bytes, executable
and schema fingerprints match. Decode failures remain searchable. Analysis reports
resource owners/dependencies, fact handoffs, connected components and cinematic
event usage. `corpus-extract <index> <source> --out <file>` re-extracts exact,
hash-verified bytes. Add `--pointer <JSON pointer>` to capture
the value and source provenance as evidence.

Scenarios replay declared sequential outcome signals, duplicate event IDs and
save/load snapshots. They check routing contracts and expectations. They do not
execute game conditions, parallel groups or engine saves. Graph previews include
the authored parallel group declarations.

`evidence-record --inputs <files...> --attachments <files...> --observations <json>
--out <receipt>` binds explicit observations to resource/tool bytes.
`evidence-verify <receipt>` reports changed/missing files and failed observations.
Gameplay evidence requires `kind: "gameplay"`, `passed: true`, bound native
resources/archive bytes and attached evidence;
it remains a supplied playtest observation, not an automated game run.
Add `stage_types` to observations and pass receipts with `catalog --evidence` to
show evidence beside the corresponding block contracts.

Races, advanced braindance production, audio generation and experimental runtime
jobs belong to separate projects. Core scene/world bindings and published-output
inputs remain available for integrating their results.

## Code layout

- `src/compiler/blocks/<block>.rs`: one procedural builder per stage kind.
- `src/compiler/blocks/shared`: shared graph and lifecycle operations.
- `src/compiler/blocks/mod.rs`: stage dispatch.
- `src/manifest`: routing, fact availability and ownership checks.
- `src/composition.rs` and `src/journal.rs`: authoring expansion and journal/localization generation.
- `src/scene`: actor acquisition, screenplay, choices, timeline, commands and performances.
- `src/world`: world placement, geometry, communities, devices and splines.
- `src/workspot.rs`: generated animation hubs and workspot entry trees.
- `src/ui.rs`, `src/fx.rs`: generated website/widget and timed visual resources.
- `src/native.rs`: fresh native serialization and semantic readback.
- `src/package`: dependency selection and verified distribution.
- `src/lifecycle`: generated reusable meeting/interaction/completion graphs.
- `src/localization.rs`: dialogue subtitle/voiceover resources for published audio.
- `src/audit`: generated-binding and supplied-authoring checks.
- `src/inspect.rs`, `src/corpus.rs`, `src/scenario.rs`, `src/evidence.rs`: inspection,
  research, routing previews and evidence.
- `src/publication.rs`, `src/install.rs`: verified replacement and scoped installation.

The library's `compiler::Inputs` collects authoring declarations;
`compiler::compile` returns native documents and loose registrations in a
`compiler::Build`. `compiler::Extension` supports generated project-specific
stages, root graphs, transforms and additional artifacts.
