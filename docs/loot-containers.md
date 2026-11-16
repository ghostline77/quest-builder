# Lootable containers

In **Locations → Loot containers**, add a container, set its position, then add
item record IDs and quantities. Drop chance ranges from 0 to 1. Use the item
catalogue or item-builder's exported record ID for the contents.

A container belongs in a world spec's `loot_containers` array. See
[the complete example](../examples/loot-cache.world.json). Include that world
spec in your build's `worlds` inputs. The compiler generates the placement,
inventory override and quest-owned loot tables together. Generated tweaks are
required package resources; no separate loot-generation command is needed.

The default appearance is a vanilla small weapon case. Other `entity_template`
values must already be loot-capable entities, with inventory, interaction and
gameplay-role components. Changing a decorative prop's loot table alone does
not give it those components.

## Placement fields

Each object in `loot_containers` accepts:

| Field | Type | Meaning |
| --- | --- | --- |
| `ref` | String | Stable world reference; use a lowercase identifier, optionally prefixed with `#`. |
| `position`, `yaw`, `sector` | Vector/anchor, number, string | Normal world placement settings. `exterior` streams independently; the default quest sector must be activated by the quest. |
| `entity_template`, `appearance` | Strings | Existing container resource and appearance. |
| `items` | Array of objects | Item `id`, optional positive integer `quantity` (default 1) and optional `chance` from 0 to 1 (default 1). |
| `loot_tables` | Array of strings | Additional existing loot-table record IDs. Can be used without `items`. |
| `locked`, `disabled`, `quest` | Booleans | Optional container state flags, all false by default. |
| `buffer_id` | Integer | Optional positive instance-buffer identity; normally assigned automatically. |

## Mesh-based containers

Advanced authors can supply `mesh` and `mesh_appearance` instead of an entity
template. This generates a new container entity with inventory, interactions,
targeting and UI slots around that mesh. It does not generate opening animations
or collision geometry. Supply mod-owned meshes through the build's asset inputs.
New entity templates are subject to the cross-platform package restrictions;
using an existing vanilla container avoids adding a new `.ent`.

## Validation

Quantities, chance values, record-ID syntax and duplicate identities are checked
at generation time. Syntax validation cannot establish that a custom item record
exists in the installed game. Install its owning mod as well. Native readback
verifies the generated resources; test interaction, contents and save persistence
in game before release.
