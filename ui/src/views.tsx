import { useState } from "react";
import { Input } from "@ghostline/ui-core";
import { ask } from "./dialogs";
import { Action, Field, Fields, Panel } from "./fields";
import {
  array,
  clone,
  human,
  kind,
  nameOf,
  object,
  parse,
  relative,
  text,
} from "./model";
import type { Json } from "./model";
import { useWorkbench, workbench as wb } from "./store";
import { createDocument, editCopy } from "./story";
import { FlowGraph, PlacementMap, Timeline } from "./graphs";
export function Library({ type }: { type: "scene" | "world" | "character" }) {
  const [query, setQuery] = useState(""),
    { questPath } = useWorkbench(),
    records = wb.documents(type, type !== "character");
  const filtered = records.filter((r) =>
    `${nameOf(r.value)} ${text(object(r.value).description)} ${r.path}`
      .toLowerCase()
      .includes(query.toLowerCase()),
  );
  return (
    <div className="space-y-5">
      <div className="flex flex-wrap items-center gap-3">
        <Input
          className="min-w-48 flex-1"
          type="search"
          aria-label={`Find ${type}`}
          placeholder={`Find ${type === "scene" ? "scenes" : type === "world" ? "locations" : "cast"}…`}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <span className="text-muted" aria-live="polite">
          {filtered.length} of {records.length}
        </span>
        {type !== "character" && (
          <Action
            variant="primary"
            disabled={!questPath}
            onClick={() => createDocument(type)}
          >
            {type === "scene" ? "Add scene" : "Add location"}
          </Action>
        )}
      </div>
      {filtered.map((r) => (
        <Action
          key={r.path}
          className="flex w-full flex-col items-start gap-2 p-5 text-left"
          onClick={() => wb.open(r.path)}
        >
          <strong className="text-base">{nameOf(r.value)}</strong>
          <span className="text-muted">
            {text(object(r.value).description)}
          </span>
          <small className="text-muted">{r.path}</small>
        </Action>
      ))}
      {!filtered.length && (
        <p className="py-10 text-center text-muted">
          {records.length
            ? "No matching entries."
            : type === "character"
              ? "Save character manifests in this project’s characters folder to make them available here."
              : "No entries yet. Choose a quest and add one to get started."}
        </p>
      )}
      {type === "scene" && (
        <Panel title="Other dialogue">
          {wb.documents("dialogue").map((r) => (
            <Action key={r.path} onClick={() => wb.open(r.path)}>
              {nameOf(r.value)}
            </Action>
          ))}
        </Panel>
      )}
    </div>
  );
}
export function Scene() {
  const { doc } = useWorkbench(),
    [tab, setTab] = useState("dialogue"),
    data = object(doc?.value);
  if (!doc || !["scene", "dialogue"].includes(kind(doc.path, doc.value)))
    return <Library type="scene" />;
  if (kind(doc.path, doc.value) === "dialogue") return <Script />;
  const manifest =
    typeof data.manifest === "object"
      ? object(data.manifest)
      : object(
          wb.records.find(
            (r) => r.path === relative(doc.path, text(data.manifest)),
          )?.value,
        );
  const nodes = [
    "sections",
    "choices",
    "quest_nodes",
    "xor_nodes",
    "end_nodes",
    "start_nodes",
  ]
    .flatMap((k) => array(data[k]))
    .concat(
      [data.start_node, data.end_node].filter(
        (v): v is Json => v !== undefined,
      ),
    )
    .map((v) => ({ value: text(object(v).key), label: nameOf(v) }));
  return (
    <div className="space-y-5">
      <RecordToolbar section="scenes" />
      <div
        role="tablist"
        aria-label="Scene views"
        className="flex flex-wrap gap-2"
      >
        {["dialogue", "choices", "cast", "graph", "timeline"].map((t) => (
          <Action
            key={t}
            role="tab"
            aria-selected={tab === t}
            variant={tab === t ? "primary" : "ghost"}
            onClick={() => setTab(t)}
          >
            {human(t)}
          </Action>
        ))}
      </div>
      {tab === "dialogue" && (
        <>
          <Action
            variant="primary"
            onClick={() => openDialogue(doc.path, data.manifest)}
          >
            Write dialogue
          </Action>
          {array(data.sections).map((s, i) => (
            <Panel key={text(object(s).key)} title={nameOf(s)} open={i === 0}>
              {array(object(s).lines).map((line, j) => {
                const key =
                    typeof line === "string" ? line : text(object(line).key),
                  wording = object(
                    array(manifest.spoken_lines).find(
                      (v) => object(v).key === key,
                    ),
                  );
                return (
                  <div
                    key={`${key}-${j}`}
                    className="flex items-start gap-3 rounded border border-line p-3"
                  >
                    <span className="text-xs text-muted">{j + 1}</span>
                    <div className="flex-1">
                      <strong>{text(wording.speaker, "Speaker")}</strong>
                      <p className="mt-1">{text(wording.text, key)}</p>
                      <Action
                        onClick={() => openDialogue(doc.path, data.manifest)}
                      >
                        Edit wording
                      </Action>
                    </div>
                    <Action
                      aria-label={`Remove line ${j + 1}`}
                      onClick={() =>
                        wb.set(["sections", i, "lines", j], undefined)
                      }
                    >
                      Remove
                    </Action>
                  </div>
                );
              })}
              <Action
                onClick={async () => {
                  const choices = array(manifest.spoken_lines).map((v) => ({
                    value: text(object(v).key),
                    label: `${text(object(v).speaker)}: ${text(object(v).text).slice(0, 80)}`,
                  }));
                  if (!choices.length)
                    throw Error(
                      "Write dialogue before adding lines to this section.",
                    );
                  const result = await ask({
                    title: "Add dialogue line",
                    fields: [{ name: "key", label: "Line", options: choices }],
                    confirm: "Add line",
                  });
                  if (result?.key)
                    wb.edit((value) => {
                      const section = object(array(object(value).sections)[i]),
                        lines = array(section.lines);
                      section.lines = lines;
                      lines.push(result.key!);
                    });
                }}
              >
                Add dialogue line
              </Action>
              <Panel title="Section settings">
                <Fields path={["sections", i]} />
              </Panel>
            </Panel>
          ))}
          <Action onClick={addSection}>Add conversation section</Action>
        </>
      )}
      {tab === "choices" && (
        <>
          {array(data.choices).map((c, i) => (
            <Panel key={text(object(c).key)} title={nameOf(c)} open>
              {array(object(c).options).map((o, j) => {
                const option = object(o),
                  wording = object(
                    array(manifest.choice_lines).find(
                      (v) => object(v).key === option.choice_key,
                    ),
                  );
                return (
                  <div key={j} className="space-y-3 border-b border-line pb-4">
                    <p>
                      {text(wording.text || option.caption, "Unwritten choice")}
                    </p>
                    <Action
                      onClick={() => openDialogue(doc.path, data.manifest)}
                    >
                      Edit wording
                    </Action>
                    <Field
                      path={["choices", i, "options", j, "target"]}
                      label="Continue to"
                      options={nodes}
                    />
                    <Panel title="Advanced option settings">
                      <Fields path={["choices", i, "options", j]} />
                    </Panel>
                  </div>
                );
              })}
              <Fields path={["choices", i]} />
            </Panel>
          ))}
          <Fields path={["choices"]} />
        </>
      )}
      {tab === "cast" && <Fields path={["actors"]} />}
      {tab === "graph" && (
        <>
          <FlowGraph scene />
          <Panel title="Scene nodes & commands">
            <Fields />
          </Panel>
        </>
      )}
      {tab === "timeline" && <Timeline />}
    </div>
  );
}
async function openDialogue(
  scenePath: string,
  manifest: Json | undefined,
): Promise<void> {
  if (typeof manifest === "string")
    await wb.open(relative(scenePath, manifest));
  else if (manifest && typeof manifest === "object")
    wb.advanced("fields", ["manifest"]);
  else throw Error("Set this scene’s dialogue manifest under Advanced.");
}
async function addSection(): Promise<void> {
  const data = object(wb.getSnapshot().doc?.value),
    result = await ask({
      title: "Add a conversation section",
      fields: [
        { name: "name", label: "Section name" },
        {
          name: "after",
          label: "Insert after",
          options: [
            { value: "", label: "Scene start" },
            ...array(data.sections).map((v) => ({
              value: text(object(v).key),
              label: nameOf(v),
            })),
          ],
        },
      ],
      confirm: "Add section",
    });
  if (!result?.name?.trim()) return;
  const base = result.name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "_")
    .replace(/^_+|_+$/g, "");
  if (!/^[a-z]/.test(base))
    throw Error("Start the section name with a letter.");
  let key = base,
    n = 2;
  while (
    ["sections", "choices", "quest_nodes", "start_nodes", "end_nodes"].some(
      (k) => array(data[k]).some((v) => object(v).key === key),
    ) ||
    object(data.start_node).key === key ||
    object(data.end_node).key === key
  )
    key = `${base}_${n++}`;
  wb.edit((value) => {
    const d = object(value),
      sections = array(d.sections),
      previous = result.after
        ? sections.find((s) => object(s).key === result.after)
        : d.start_node || array(d.start_nodes)[0];
    if (!previous) throw Error("Add a start node in the scene graph first.");
    const route = result.after ? "on_end" : "on_start";
    const next = {
      key,
      lines: [],
      on_end: clone(array(object(previous)[route])),
    };
    object(previous)[route] = [{ node: key }];
    d.sections = sections;
    sections.splice(
      result.after
        ? sections.findIndex((s) => object(s).key === result.after) + 1
        : 0,
      0,
      next,
    );
  });
}
function Script() {
  const { doc } = useWorkbench(),
    data = object(doc?.value),
    speakerNames = new Set([
      "V",
      ...wb.documents("character", false).map((r) => nameOf(r.value)),
      ...array(data.spoken_lines)
        .map((l) => text(object(l).speaker))
        .filter(Boolean),
    ]);
  return (
    <div className="space-y-6">
      <RecordToolbar section="scenes" />
      {(["spoken_lines", "choice_lines"] as const).map((collection) => (
        <section key={collection} className="space-y-4">
          <h2 className="text-lg font-semibold">
            {collection === "spoken_lines"
              ? "Spoken lines"
              : "Player choice wording"}
          </h2>
          {array(data[collection]).map((line, i) => (
            <article
              key={text(object(line).key, String(i))}
              className="flex gap-4 rounded-lg border border-line p-4"
            >
              <span className="text-muted">
                {String(i + 1).padStart(2, "0")}
              </span>
              <div className="flex-1 space-y-4">
                {collection === "spoken_lines" && (
                  <Field
                    path={[collection, i, "speaker"]}
                    label={`Speaker · line ${i + 1}`}
                    options={[...speakerNames].map((v) => ({
                      value: v,
                      label: v,
                    }))}
                  />
                )}
                <Field
                  path={[collection, i, "text"]}
                  label={`${collection === "spoken_lines" ? "Dialogue" : "Choice wording"} · line ${i + 1}`}
                />
                <Panel title="Advanced line settings">
                  <Fields path={[collection, i]} />
                </Panel>
              </div>
            </article>
          ))}
          <Action
            onClick={async () => {
              const result = await ask({
                title:
                  collection === "spoken_lines"
                    ? "Add spoken line"
                    : "Add choice wording",
                fields: [
                  ...(collection === "spoken_lines"
                    ? [
                        {
                          name: "speaker",
                          label: "Speaker",
                          options: [...speakerNames].map((v) => ({
                            value: v,
                            label: v,
                          })),
                        },
                      ]
                    : []),
                  { name: "text", label: "Text", type: "textarea" },
                ],
                confirm: "Add line",
              });
              if (!result?.text?.trim()) return;
              if (collection === "spoken_lines" && !result.speaker)
                throw Error("Choose who speaks the line.");
              wb.edit((value) => {
                const d = object(value),
                  rows = array(d[collection]);
                d[collection] = rows;
                let key = collection === "spoken_lines" ? "line" : "choice",
                  n = 2;
                const base = key;
                while (rows.some((v) => object(v).key === key))
                  key = `${base}_${n++}`;
                rows.push({
                  key,
                  ...(result.speaker ? { speaker: result.speaker } : {}),
                  text: result.text!.trim(),
                });
              });
            }}
          >
            Add{" "}
            {collection === "spoken_lines" ? "spoken line" : "choice wording"}
          </Action>
        </section>
      ))}
    </div>
  );
}
function RecordToolbar({
  section,
}: {
  section: "scenes" | "locations" | "cast";
}) {
  return (
    <div className="flex flex-wrap gap-3">
      <Action
        onClick={async () => {
          await wb.flush();
          wb.update({ doc: null, selection: [], section });
        }}
      >
        All {section}
      </Action>
      <Action onClick={editCopy}>Edit title & description</Action>
      <Action onClick={() => wb.advanced("fields")}>All fields</Action>
    </div>
  );
}
export function Locations() {
  const { doc } = useWorkbench(),
    data = object(doc?.value);
  if (!doc || kind(doc.path, doc.value) !== "world")
    return <Library type="world" />;
  return (
    <div className="space-y-5">
      <RecordToolbar section="locations" />
      <Action variant="primary" onClick={importPosition}>
        Use captured position
      </Action>
      <Panel title="Starting position · world coordinates" open>
        <Fields path={["origin"]} />
      </Panel>
      {[
        "markers",
        "triggers",
        "entities",
        "loot_containers",
        "devices",
        "communities",
      ].map((key) => (
        <Panel key={key} title={`${human(key)} · ${array(data[key]).length}`}>
          <Fields path={[key]} />
        </Panel>
      ))}
      {data.community && (
        <Panel title="Characters at this location">
          <Fields path={["community"]} />
        </Panel>
      )}
      <Panel title="Placement map">
        <PlacementMap key={doc.path} />
      </Panel>
    </div>
  );
}
export async function importPosition(): Promise<void> {
  const result = await ask({
    title: "Use a captured game position",
    message: "Paste x, y, z and optional yaw to set the placement origin.",
    fields: [
      { name: "source", label: "Captured position JSON", type: "textarea" },
    ],
    confirm: "Set origin",
  });
  if (!result?.source) return;
  const source = object(parse(result.source)),
    p = object(source.position || object(source.camera).position || source);
  const origin = {
    x: Number(p.x ?? p.X),
    y: Number(p.y ?? p.Y),
    z: Number(p.z ?? p.Z),
    yaw: Number(source.yaw ?? p.yaw ?? object(source.camera).yaw ?? 0),
  };
  if (Object.values(origin).some((v) => !Number.isFinite(v)))
    throw Error(
      "A captured position must contain finite x, y and z coordinates.",
    );
  wb.set(["origin"], origin);
}
export function Cast() {
  const { doc, settings } = useWorkbench(),
    data = object(doc?.value);
  if (!doc || kind(doc.path, doc.value) !== "character")
    return <Library type="character" />;
  const ids = [
    data.id,
    data.name,
    object(data.identity).record,
    object(data.identity).voice_tag,
  ]
    .filter((v): v is string => typeof v === "string")
    .map((s) => s.toLowerCase());
  const scenes = wb
    .documents("scene")
    .filter((r) =>
      array(object(r.value).actors).some((a) =>
        ["key", "name", "entry", "record"].some((k) =>
          ids.includes(text(object(a)[k]).toLowerCase()),
        ),
      ),
    );
  return (
    <div className="space-y-5">
      <RecordToolbar section="cast" />
      {settings.characterBuilderUrl && (
        <a
          className="inline-flex rounded bg-accent px-4 py-2 text-accent-ink"
          href={settings.characterBuilderUrl}
          target="_blank"
          rel="noopener noreferrer"
        >
          Open character-builder
        </a>
      )}
      <Panel title="Character" open>
        <Field path={["name"]} label="Name" />
        <p className="text-muted">Appears in</p>
        {scenes.map((r) => (
          <Action key={r.path} onClick={() => wb.open(r.path)}>
            {nameOf(r.value)}
          </Action>
        ))}
        {!scenes.length && (
          <p className="text-muted">Not cast in a scene in this quest yet.</p>
        )}
      </Panel>
      <Panel title="Appearance & clothing" open>
        <Field
          path={["active_appearance"]}
          label="Default appearance"
          options={array(data.appearances).map((a) => ({
            value: text(object(a).id),
            label: nameOf(a),
          }))}
        />
        <Fields path={["appearances"]} />
      </Panel>
      <Panel title="Weapons & loadout">
        <Fields path={["loadout"]} />
      </Panel>
      <Panel title="NPC behavior">
        <Fields path={["ai"]} />
      </Panel>
    </div>
  );
}
