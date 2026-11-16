# Named scene authoring

Scene specifications describe nodes and connections by name. The compiler
allocates the numeric node, actor, prop, workspot and local animation-reference
IDs required by native resources.

See the [meeting scene](../examples/meeting.scene-spec.json) and
[its dialogue](../examples/meeting.dialogue.json):

```powershell
target/release/ghostline-quest-builder.exe scene examples/meeting.scene-spec.json --dialogue examples/meeting.dialogue.json --out work/meeting-scene
```

## Nodes and connections

Every start/end node, section, choice, quest command, XOR and executable
performance block has a unique `key` within the scene. Keys use lower-case
letters, digits and underscores, beginning with a letter.

```json
{
  "key": "greeting",
  "lines": ["welcome"],
  "on_end": [{"node": "enable_terminal"}]
}
```

`node` references a node's key. Omitted `input` means `In`; specify it only for
another socket, such as `Active`, `Inactive` or `Succeeded`. Ordinary flow cannot
enter `CutDestination`. Choice options use `target`:

```json
{
  "key": "job_choice",
  "actor": "contact",
  "options": [{"choice_key": "accept", "target": "accept_job"}]
}
```

Entry/exit declarations use `node` to bind public scene names to start/end nodes.
Those public names form the interface called by quest stages. Use either a
singular `start_node`/`end_node`/`entry_point` declaration or its plural form,
never both. Graph ordering is normally automatic; optional `graph_order` lists
node keys.

## Time windows

For an appointment, `time_period` waits for a daily window expressed as
`begin_seconds` and `end_seconds` (inclusive, 0–86399). Add `trigger_ref` to
require the player to be inside an arrival trigger during that window.
Choice options accept `available_time` with the same two fields to limit when
an interaction appears. `set_time` sets `hours`, `minutes` and `seconds` under
an authored `source`; surround it with `render_fade` and release any scene
controls before exiting the wait sequence.

## Companion movement

For an NPC travelling with V, `move_on_spline` takes `entity_ref`, `entry`,
`spline_ref` and optional `reverse`. It emits SQ017's `WithCompanion` movement:
local player companion, `Close` distance, no catch-up or teleport, and no
start/stop animation. Route points belong to the world specification.

## Elevator sequences

An elevator scene can follow movement with `use_scene_workspot` for its cabin
idle. `pause_condition` defaults to V; supply `entity_ref` and `entry` to check
the NPC separately. `elevator_attachment` takes the NPC, `elevator_ref` and
`attach`. `device_action` and `device_condition` take `device_ref`,
`controller_class`, `action` or `condition`, and optional typed `properties`
(the same `{ "type": "Int32", "value": 1 }` shape used by scope actions).
Keep movement, cabin idle and attachment in the scene until arrival: check
both occupants before closing doors and sending `GoToFloor`, wait for
`IsAtFloor`, detach, open the doors, stop the cabin workspot and start the
outbound companion movement. A broad player proximity trigger does not prove
that the elevator reached its destination.

## Actors, props and workspots

Actors and props have unique keys. Choices reference actors with `actor`;
dialogue, gaze and performance roles also use actor names. The compiler reserves
the native player actor/performer identities automatically. Actor `lipsync` is a
boolean: community/player actors default to enabled, scene-spawned actors to
disabled.

Scene-local workspots have keys. A performance root-motion binding uses
`workspot: "chair"` to reference the declared `chair` workspot. Command
`workspot_ref` selects a placed world workspot by NodeRef; use `"current"` for
the actor's current workspot.

For an NPC placed by a population spawner, use actor `kind: "spawner"` and
`spawner_ref` with the world spawner's NodeRef. `force_max_visibility` defaults to
true. Commands targeting that NPC use `entity_ref` with the same NodeRef and
omit the community `entry`. `wait_actor_spawned` accepts this direct reference;
set `present: false` to wait for removal. It cannot also use `dynamic_name`.

The `play_scene` quest stage can activate NPC `spawners` before scene entry and
wait until they exist. Set `despawn_after: true` to deactivate them after the
scene exits and wait for removal. Its scene sockets default to `start` and
`end`; optional `wait_trigger` delays spawning until the player approaches.
See the [quest-owned world activity example](../examples/native-workspots/README.md).

Generated workspot entry trees use unique keys for sequences, clips, pauses and
random lists. Native entry IDs and the counter are generated. World community
registry/activity IDs derive from named world references; no separate numeric
registry IDs are authored.

## Animation references

RID resources have `key` and `path`. RID body/camera animations have `key`, a named
`resource`, and the published asset's `serial_number`. Body/facial sets have keys
and list animation keys in `animations`; actors reference set keys. Section RID
events and camera events select an `animation` by name.

```json
{
  "rid_resources": [{"key": "performance", "path": "mod\\example\\performance.scenerid"}],
  "rid_animations": [{"key": "contact_pose", "resource": "performance", "serial_number": 7}],
  "rid_anim_sets": [{"key": "contact_body", "animations": ["contact_pose"]}]
}
```

Resource IDs and table indices are internal to compilation. Duplicate keys,
unknown names, invalid socket names and numeric internal-ID fields fail with a
diagnostic.

## References into existing assets

Some numbers identify data outside the generated scene and retain that identity:

- Dialogue `string_id` values bind published subtitles, audio and lipsync clips.
- RID `serial_number` selects an animation inside a published `.scenerid`.
- Workspot `entry_id`, `exit_entry_id` and `sequence_entry_id` select entries in
  the referenced game/published workspot.
- Controller component IDs bind overrides to an existing entity component.
- Voice tags, TweakDB record IDs, masks and enum values describe existing game
  data or behavior.

Preserve these external bindings. Numeric internal scene IDs appear only in
generated resources and are never used to connect authoring nodes.

## Interesting conversations

A world `conversations` entry can set `registered_scene` to an existing native
conversation scene path. Its `scene` remains the generated resource path. The
compiler also emits the generated scene at the registered path and makes the
area request that registered path.
This replaces every use of that registered path; choose a registration whose
original scene is not used by gameplay or another mod.

Native builds require `--game` to check admission metadata. Context actors and
their workspot reference hashes must match the selected registration. A custom
scene path alone does not register an interesting conversation. No global audio
metadata override or ArchiveXL resource patch is emitted by this mechanism.

## Quest bootstrap

This section describes the standalone native bootstrap. It is not required for
normal PC ArchiveXL quest registration, and Quest Core slot packs must not each
enable it. Choose the arrangement in [quest startup](reference.md#quest-startup)
before using these settings.

Set `resources.bootstrap` to `true` on a quest to start its root through a native
map-wide conversation area, independently of H10. Generation requires `--game`
with bundled reflection metadata. The compiler generates the area and scene,
uses a compatible registered vendor conversation, keeps its sector loaded and isolates a forced
spawn phase for that vendor, reusing its existing workspot.
The scene signals a request fact. A native global quest host receives it and
launches the root phase once; the quest remains independent of the scene's lifetime.
Generated NPC workspots are merged into the vanilla world's initial workspot
batch in ID order; custom registry nodes do not append them again at runtime.
Finite roots use `In1`/`Out1`; a completion fact prevents restarting finished quests.

Rust callers can use `bootstrap::attach` with a `bootstrap::Spec`. Set a phase's
`output` to its completion socket, or leave it absent for a persistent controller
such as a replayable race championship. Persistent controllers own their durable
progression and resume through their saved facts.

One package owns the registered scene, two vanilla sector overrides and the
persistent quest host override. Aggregate quests under that package's controller.
Independently packed bootstraps that override these same paths conflict.
Quest Core slot integrations use a shared core for these overrides; see
[quest startup](reference.md#quest-startup). Generated build evidence lists
the bootstrap and binary hashes. Console runtime validation is still required.
