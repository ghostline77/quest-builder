# Quest-owned world workspot

This example spawns a vanilla NPC when the player approaches a pavement activity,
starts a standing workspot, then releases and despawns the NPC when the player
enters the separate leave trigger. All resources are generated from the quest,
scene and world specifications here.

```powershell
target/release/ghostline-quest-builder.exe compile examples/native-workspots/quest.json --inputs examples/native-workspots/inputs.json --out work/world-activity --game "X:/Cyberpunk 2077"
```

Choose a new output directory. This compiles resources; connect the generated
root to your quest controller or allocated Quest Core slot and package the
streaming block through your normal native build.

The example places the activity on Pacifica pavement near X=-2856, Y=-2719.
The approach trigger spans 60 metres. Walk to X=-2898, Y=-2709 to end it.
Change the namespace and depot paths for your own quest. Measure the ground
height separately for the NPC spawn and the workspot; `snap_to_ground` is not
a substitute for correct coordinates when teleporting an actor into a spot.

The population spawner owns the NPC. The scene acquires that existing NPC using
actor `kind: "spawner"`; it does not create a scene-owned replacement. Direct
`entity_ref` commands omit a community entry name. `play_scene` waits for NPC
creation before entering the scene and, with `despawn_after`, waits for removal
after the scene exits. Stop the workspot before exiting the scene.

These world records use `sector: "always_loaded"` so the approach trigger and
spawner exist before the stage starts. Larger quests can manage streaming
through their own phase prefabs instead. No persistent community workspot
registration or core workspot-bank allocation is needed for this direct use.
Ambient community schedules still require registered community workspots.
