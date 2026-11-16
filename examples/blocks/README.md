# Quest block manifests

One JSON quest manifest for each supported stage kind. See the
[building block guide](../../docs/building-blocks.md) for the grouped index,
binding requirements, option explanations and generation commands.

Run commands from the repository root:

```powershell
target/release/ghostline-quest-builder.exe validate examples/blocks/meet_contact.quest.json
```

The manifests demonstrate block authoring. Bind their symbolic world, journal,
record, scene and child-phase references in the owning quest before producing
a native package.
