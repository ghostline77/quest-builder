# Building block examples

Each supported stage kind has a [JSON quest manifest](../examples/blocks/).
Every file contains one stage inside the normal versioned quest format. They use
the same schema and compiler as a larger quest.

## Use an example

Choose a block below, copy its stage into your quest's `stages` array and give it
a unique stage ID and phase depot. Keep the containing manifest when validating
or generating the example on its own:

Build the executable using the [getting-started guide](getting-started.md),
then run from the repository root:

```powershell
target/release/ghostline-quest-builder.exe validate examples/blocks/meet_contact.quest.json
target/release/ghostline-quest-builder.exe plan examples/blocks/meet_contact.quest.json
```

Validation and planning do not generate game resources. The examples use symbolic
`#example_*` world references, journal paths and mod-owned records/scenes/phases
to show each binding explicitly. Supply the referenced authoring through your
quest's `resources` section and define the item/character/vehicle records before
making a native build or package. `status: "ready"` means the stage's authoring
is ready for generation; it does not establish that its external resources
exist or that the resulting quest has been playtested.

The time-gate example has no external bindings. Generate its resource documents
without native binaries:

```powershell
target/release/ghostline-quest-builder.exe compile examples/blocks/time_gate.quest.json --documents-only --out work/examples/time-gate-documents
```

For a native build, omit `--documents-only`:

```powershell
target/release/ghostline-quest-builder.exe compile examples/blocks/time_gate.quest.json --out work/examples/time-gate-native
```

Output directories must be new. Replace a prior verified build through
`replace-build`, or choose another output directory.

## Read the fields

- `id` identifies this stage inside the quest; `type` selects its building block.
- `phase_resource` is the generated native child-phase depot.
- `objective`, `description_entry` and `mappin` reference journal content.
  Define that content through quest composition or your owning project's journals.
- Facts are persistent named state. A waiting block needs the named producer or
  gameplay signal; including a condition fact does not automatically set it.
- NodeRefs identify authored world objects or communities. The `entry` inside a
  community is its named actor/vehicle entry.
- `inputs` and `outcomes` map authoring aliases to native socket names.
  `on` routes an outcome to another stage or `$end`. Omitting those maps uses
  the block's default single start/success flow.
- `child`, `application`, `report` and `routes` bind generated phases or
  authored scenes with exact entry/exit sockets. Their exit maps associate an
  authoring outcome with the called native socket.

The examples deliberately show useful option combinations rather than every
optional field. Use `schema` for the quest authoring JSON schema and
`catalog` for required fields, ports and defaults:

```powershell
target/release/ghostline-quest-builder.exe schema --out work/quest-schema.json
$blockExamples = (Get-ChildItem examples/blocks -Filter '*.quest.json').FullName
target/release/ghostline-quest-builder.exe catalog --examples $blockExamples --markdown --out work/block-reference.md
```

## Objectives, items and investigation

| Manifest | Demonstrates | Bind in the owning quest |
| --- | --- | --- |
| [`reach_area`](../examples/blocks/reach_area.quest.json) | Activate an objective and map pin until the player enters its trigger. | Trigger, objective, description and pin. |
| [`leave_area`](../examples/blocks/leave_area.quest.json) | Finish after leaving a trigger and deactivate an owned community. | Exit trigger and owned community. |
| [`acquire_item`](../examples/blocks/acquire_item.quest.json) | Wait for an item to enter the player's inventory. | Item record and objective. |
| [`read_shard`](../examples/blocks/read_shard.quest.json) | Wait for item acquisition and a journal readable's read state. | Item record and readable journal entry. |
| [`read_terminal_document`](../examples/blocks/read_terminal_document.quest.json) | Power a computer and wait for its journal document to be read. | Placed computer and document journal entry. |
| [`investigate_clues`](../examples/blocks/investigate_clues.quest.json) | Scan two independent clues in either order. | Two scannable objects and journal objective. |
| [`investigate_reactions`](../examples/blocks/investigate_reactions.quest.json) | Track scan and reaction completion separately, including resumed progress. | Scannable objects, reaction scenes and persistent clue facts. |
| [`optional_condition`](../examples/blocks/optional_condition.quest.json) | Evaluate a fact immediately and record optional success or failure. | A fact written by the surrounding quest or game event. |
| [`stealth_monitor`](../examples/blocks/stealth_monitor.quest.json) | Race failure facts against a stop signal and record an optional result. | External failure/stop facts and optional objective. |

## Contacts and conversation

| Manifest | Demonstrates | Bind in the owning quest |
| --- | --- | --- |
| [`phone_job_offer`](../examples/blocks/phone_job_offer.quest.json) | Delay a phone offer and wait for its acceptance choice. | Contact, phone messages and acceptance choice. |
| [`phone_conversation`](../examples/blocks/phone_conversation.quest.json) | Show messages, offer two replies, send a final message and record completion. | Contact, thread, messages, choice group and both reply pairs. |
| [`phone_call`](../examples/blocks/phone_call.quest.json) | Start an audio call between journal contacts. | Both contacts and the holocall prefab. |
| [`play_scene`](../examples/blocks/play_scene.quest.json) | Play a scene, optionally spawning NPCs first and despawning them afterward. | Scene, origin marker, optional approach trigger and population spawners. See the [complete example](../examples/native-workspots/README.md). |
| [`meet_contact`](../examples/blocks/meet_contact.quest.json) | Spawn a contact and rearm a deferred meeting after the player leaves. | Community actor, scene, setup/rearm triggers, objective and pin. |
| [`contact_fallback`](../examples/blocks/contact_fallback.quest.json) | Try a call and deliver a message when the contact is unavailable or rejects it. | Call scene with connected/rejected exits and fallback message. |

## Devices and delivery

| Manifest | Demonstrates | Bind in the owning quest |
| --- | --- | --- |
| [`interact_device`](../examples/blocks/interact_device.quest.json) | Observe a terminal predicate without sending its action automatically. | Device controller and completion predicate. |
| [`hack_access_point`](../examples/blocks/hack_access_point.quest.json) | Send an access-point action and wait for its hacking-success predicate. | Access-point controller and hacking-success predicate. |
| [`plant_item`](../examples/blocks/plant_item.quest.json) | Observe personal-link installation while retaining the required tracker item. | Tracker record, access point and personal-link predicate. |
| [`deliver_drop_point`](../examples/blocks/deliver_drop_point.quest.json) | Reserve a drop point for an item and wait for its deposit fact. | Item record, drop point and externally written deposit fact. |
| [`access_routes`](../examples/blocks/access_routes.quest.json) | Race access attempts, cancel losing work and run a shared application. | Two generated attempt phases and a generated application phase. |
| [`acquire_or_use`](../examples/blocks/acquire_or_use.quest.json) | Recognize existing possession or wait for acquisition before running an application. | Access-key record and generated application phase. |

## Actors and combat

| Manifest | Demonstrates | Bind in the owning quest |
| --- | --- | --- |
| [`community`](../examples/blocks/community.quest.json) | Activate a named community entry and set its attitude group. | Community with a named guard entry. |
| [`escort_npc`](../examples/blocks/escort_npc.quest.json) | Assign a follower and monitor three route gates. | Actor, three destination triggers and pin. |
| [`carry_npc`](../examples/blocks/carry_npc.quest.json) | Observe a carried NPC passing waypoints and reaching a delivery area. | Carried actor, waypoint triggers and destination. |
| [`release_or_rescue_npc`](../examples/blocks/release_or_rescue_npc.quest.json) | Unlock a device and wait for rescued community entries. | Door controller and three community entries. |
| [`defend_target`](../examples/blocks/defend_target.quest.json) | Monitor a protected actor until an external completion signal or actor failure. | Protected actor and external defense-completion fact. |
| [`combat_encounter`](../examples/blocks/combat_encounter.quest.json) | Activate named combatants and finish when both are defeated, allowing nonlethal outcomes. | Community with two named actors. |
| [`cyberpsycho_encounter`](../examples/blocks/cyberpsycho_encounter.quest.json) | Reveal a boss, distinguish spared and killed outcomes and clean up outside. | Boss record/community, outer/reveal/arena/cleanup triggers. |
| [`resolve_and_report`](../examples/blocks/resolve_and_report.quest.json) | Resolve a target under an explicit outcome policy before reporting. | Target actor and generated report phase. |
| [`observe_lethal`](../examples/blocks/observe_lethal.quest.json) | Record lethal outcomes independently until an explicit stop fact. | Target actor and external observation-stop fact. |

## Vehicles

| Manifest | Demonstrates | Bind in the owning quest |
| --- | --- | --- |
| [`enter_vehicle`](../examples/blocks/enter_vehicle.quest.json) | Wait for the player to mount a named community vehicle. | Named vehicle community entry and journal pin. |
| [`ride_with_contact`](../examples/blocks/ride_with_contact.quest.json) | Assign passengers to distinct seats and wait for everyone to mount. | Vehicle, three actors and distinct native seat slots. |
| [`drive_to`](../examples/blocks/drive_to.quest.json) | Require a named vehicle to pass waypoints and reach its destination. | Vehicle, two waypoint triggers and destination. |
| [`steal_vehicle`](../examples/blocks/steal_vehicle.quest.json) | Wait for the player to enter a vehicle and record its acquisition. | Named vehicle community entry and pin. |
| [`deliver_vehicle`](../examples/blocks/deliver_vehicle.quest.json) | Wait for a vehicle inside a delivery area, stopped, with the player outside. | Placed vehicle and destination trigger. |
| [`vehicle_cleanup`](../examples/blocks/vehicle_cleanup.quest.json) | Disable a quest vehicle entitlement and deactivate temporary communities. | Player vehicle record and temporary communities. |

## Routing, timing and owned work

| Manifest | Demonstrates | Bind in the owning quest |
| --- | --- | --- |
| [`choice_gate`](../examples/blocks/choice_gate.quest.json) | Wait for a fact-based choice, set its result fact and join the route. | Two external branch-condition facts. |
| [`time_gate`](../examples/blocks/time_gate.quest.json) | Wait one game hour and set a completion fact. | No external resource binding. |
| [`adaptive_timer`](../examples/blocks/adaptive_timer.quest.json) | Choose a weighted delay, select a fact-based profile and defer while busy. | Profile-selection and busy facts. |
| [`owned_scope`](../examples/blocks/owned_scope.quest.json) | Own child work and a progress notification, acknowledge restoration and commit. | Generated work/restoration phases and prefab variant. |
| [`reward_commit`](../examples/blocks/reward_commit.quest.json) | Check possession, grant a missing reward, verify it and commit durably. | Item reward record and external reported fact. |
| [`entitlement_reconcile`](../examples/blocks/entitlement_reconcile.quest.json) | Check entitlement and retire an obsolete offer when already owned. | Entitlement fact and offer journal/message entries. |

## Specialist integration

| Manifest | Demonstrates | Bind in the owning quest |
| --- | --- | --- |
| [`braindance_analysis`](../examples/blocks/braindance_analysis.quest.json) | Call an authored analysis scene, count clue facts and restore the player on exit. | Published analysis scene, player anchors and clue facts. |

## Important combinations

`meet_contact` explicitly supplies its setup trigger, scene origin and accepted
fact. Its `defer_policy: "rearm"` waits for the rearm area to be left before
offering the meeting again. Scene entry and accept/defer exit names must match
the authored scene.

`optional_condition` demonstrates immediate evaluation with renamed native
start/result sockets. Continuous and at-exit evaluation are other supported
policies; at-exit evaluation additionally needs `condition.stop_fact` to close
its evaluation window.

`defend_target` uses an externally written completion fact. Add
`duration_seconds` for timed defense. Escort and defense can declare separate
failure/cancelled outcomes, actor lifecycle policies and cleanup.

`owned_scope` treats `Progress` as a non-terminating child notification.
The work phase must expose that notification and a terminating `Out1`.
The restoration phase must acknowledge completion before success effects and
the durable commit. Both phases are generated from authoring. Neither a copied
phase file nor a donor template is part of the example.

`contact_fallback` exposes `connected` and `fallback`, while
`entitlement_reconcile` exposes `owned` and `unowned`. Their manifests show
the corresponding native socket aliases and explicit terminal routing. Replace
`$end` with the next stage ID when composing a larger quest.

`reward_commit` demonstrates an item-possession reward with a durable commit
fact. Its example token is a custom item record to define in TweakXL. This
possession policy is suitable for an item reward; use the reward record support
when authoring a separate cash payout.

`braindance_analysis` covers the core integration of a supplied analysis scene,
clue signals and player restoration. Advanced braindance production remains in
its separate tool.
