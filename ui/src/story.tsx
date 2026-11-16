import { useRef, useState } from "react";
import { Input, Select } from "@ghostline/ui-core";
import { ask } from "./dialogs";
import { Action, Field, Fields, Panel, move } from "./fields";
import { array, at, clone, human, nameOf, object, text } from "./model";
import type { Json, JsonObject, Path } from "./model";
import { useWorkbench, workbench as wb } from "./store";
import { families, stages, labels } from "./stage-fields";
import { FlowGraph } from "./graphs";
const stageConfigs = stages as Record<
  string,
  { name: string; fields: string[]; description: string }
>;
const fieldLabels = labels as Record<string, string>;
export function stageName(stage: JsonObject, quest: JsonObject): string {
  const id = text(stage.objective).match(/^@objectives\.([^.]+)\.path$/)?.[1];
  const objective = id
    ? object(object(quest.composition).objectives)[id]
    : undefined;
  return text(
    typeof objective === "string" ? objective : object(objective).text,
    text(
      stage.title,
      stageConfigs[text(stage.type)]?.name || human(text(stage.type)),
    ),
  );
}
export function referenceChoices(
  key: string,
): { value: string; label: string }[] {
  const q = object(wb.quest?.value),
    composition = object(q.composition),
    result: { value: string; label: string }[] = [];
  const add = (value: Json | undefined, label?: string) => {
    if (
      typeof value === "string" &&
      value &&
      !result.some((o) => o.value === value)
    )
      result.push({ value, label: label || value });
  };
  if (key === "contact") {
    for (const [id, item] of Object.entries(object(composition.contacts)))
      add(`@contacts.${id}.contact`, text(object(item).name, human(id)));
    for (const r of wb.documents("character", false))
      add(object(r.value).id, nameOf(r.value));
  } else if (key === "scene")
    for (const r of wb.documents("scene"))
      add(object(r.value).archive_path, nameOf(r.value));
  else if (key === "objective")
    for (const [id, item] of Object.entries(object(composition.objectives)))
      add(
        `@objectives.${id}.path`,
        typeof item === "string" ? item : text(object(item).text, human(id)),
      );
  else if (
    /community|trigger|origin|destination|device|computer|drop_point|target/.test(
      key,
    )
  ) {
    const category = /community/.test(key)
      ? "communities"
      : /trigger/.test(key)
        ? "triggers"
        : /device|computer|drop_point/.test(key)
          ? "devices"
          : "markers";
    for (const r of wb.documents("world")) {
      const world = object(r.value),
        rows =
          category === "communities"
            ? [
                ...(world.community ? [world.community] : []),
                ...array(world.communities),
              ]
            : array(world[category]);
      for (const row of rows) {
        const ref = object(row).ref,
          alias = Object.entries(object(composition.locations)).find(
            ([, v]) => object(v).ref === ref,
          )?.[0];
        add(alias ? `@locations.${alias}.ref` : ref, nameOf(row));
      }
    }
  } else if (
    ["thread", "message", "messages", "choice_group", "accept_choice"].includes(
      key,
    )
  ) {
    const entries = (items: Json | undefined, prefix: string): void => {
      for (const [id, item] of Object.entries(object(items))) {
        const v = object(item),
          path = `${prefix}.${id}`;
        if (v.text || v.title || v.type === "choice_group")
          add(`@${path}.path`, text(v.text || v.title, human(id)));
        entries(v.entries, path + ".entries");
      }
    };
    for (const [id, contact] of Object.entries(object(composition.contacts)))
      for (const [thread, row] of Object.entries(
        object(object(contact).threads),
      )) {
        if (key === "thread")
          add(
            `@contacts.${id}.threads.${thread}.path`,
            text(object(row).title, human(thread)),
          );
        else
          entries(
            object(row).entries,
            `contacts.${id}.threads.${thread}.entries`,
          );
      }
  }
  return result;
}
export async function createDocument(
  type: "quest" | "scene" | "world",
): Promise<void> {
  const result = await ask({
    title:
      type === "quest"
        ? "Create a quest"
        : type === "scene"
          ? "Add a scene"
          : "Add a location",
    fields: [{ name: "name", label: "Name" }],
    confirm: "Create",
  });
  if (result?.name?.trim()) await wb.create(type, result.name.trim());
}
export function Story() {
  const { doc, questPath, catalog, root } = useWorkbench();
  const [mode, setMode] = useState("outline"),
    [opened, setOpened] = useState("");
  const recipeCache = useRef(new Map<string, JsonObject>());
  const q = object(doc?.value),
    all = array(q.stages),
    key = `${root}:${questPath}`;
  // Keep reviewed meeting bindings available when a stage changes location or character.
  for (const row of all) {
    const stage = object(row);
    if (
      stage.type === "meet_contact" &&
      stage.contact &&
      stage.community &&
      stage.scene
    )
      recipeCache.current.set(
        `${key}:${JSON.stringify([stage.contact, stage.community, stage.scene])}`,
        clone(stage),
      );
  }
  const recipes = [...recipeCache.current]
    .filter(([k]) => k.startsWith(key + ":"))
    .map(([, v]) => v);
  if (!doc || doc.path !== questPath || !q.stages)
    return (
      <div className="space-y-4">
        {wb.documents("quest", false).map((r) => (
          <Action
            key={r.path}
            className="flex w-full flex-col items-start p-5 text-left"
            onClick={() => wb.open(r.path)}
          >
            <strong>{nameOf(r.value)}</strong>
            <span className="text-muted">
              {text(
                object(r.value).description,
                `${array(object(r.value).stages).length} stages`,
              )}
            </span>
          </Action>
        ))}
        <Action variant="primary" onClick={() => createDocument("quest")}>
          Create a quest
        </Action>
      </div>
    );
  const addStage = async (): Promise<void> => {
    const choices = families.flatMap((f) =>
      f.kinds
        .filter((type) => catalog.blocks.some((b) => b.type === type))
        .map((type) => ({
          value: type,
          label: `${f.name} · ${stageConfigs[type]?.name || human(type)}`,
        })),
    );
    for (const block of catalog.blocks)
      if (!choices.some((o) => o.value === block.type))
        choices.push({ value: block.type, label: human(block.type) });
    const result = await ask({
      title: "Add a stage",
      fields: [
        { name: "type", label: "Stage type", options: choices },
        { name: "name", label: "Stage name" },
      ],
      confirm: "Add stage",
    });
    if (!result?.type) return;
    const name =
      result.name?.trim() ||
      stageConfigs[result.type]?.name ||
      human(result.type);
    const base = name
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "_")
      .replace(/^_+|_+$/g, "");
    let id = /^[a-z]/.test(base) ? base : "stage_" + base,
      n = 2;
    while (all.some((row) => object(row).id === id)) id = `${base}_${n++}`;
    const block = catalog.blocks.find((b) => b.type === result.type);
    wb.edit((value) => {
      const data = object(value),
        stage: JsonObject = {
          id,
          title: name,
          type: result.type!,
          status: "ready",
          phase_resource: `mod\\${text(data.id)}\\phases\\${id}.questphase`,
        };
      for (const [field, spec] of Object.entries(block?.fields || {}))
        if (spec.required && !(field in stage))
          stage[field] = /fact$/.test(field)
            ? `${text(data.id)}_${id}_${field.replace(/_?fact$/, "")}`
            : wb.default(spec.schema, field);
      if (block?.fields.objective?.required) {
        const composition = object(data.composition),
          objectives = object(composition.objectives);
        data.composition = composition;
        composition.objectives = objectives;
        const objective: JsonObject = { text: name, description: name };
        objectives[id] = objective;
        stage.objective = `@objectives.${id}.path`;
        if (block.fields.description_entry?.required)
          stage.description_entry = `@objectives.${id}.description_entry`;
        if (block.fields.mappin?.required) {
          objective.mappins = { location: { ref: "", text: name } };
          stage.mappin = `@objectives.${id}.mappins.location.path`;
        }
      }
      const rows = array(data.stages);
      data.stages = rows;
      rows.push(stage);
    });
    setOpened(id);
  };
  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-center gap-3">
        <div className="flex gap-1 rounded border border-line p-1">
          {["outline", "graph"].map((m) => (
            <Action
              key={m}
              variant={mode === m ? "primary" : "ghost"}
              aria-pressed={mode === m}
              onClick={() => setMode(m)}
            >
              {human(m)}
            </Action>
          ))}
        </div>
        <Action onClick={editCopy}>Edit title & description</Action>
        <Action className="ml-auto" variant="primary" onClick={addStage}>
          + Add stage
        </Action>
      </div>
      {mode === "graph" ? (
        <>
          <FlowGraph inspect={false} />
          <p className="text-muted">Select a stage to edit it below.</p>
          {all.map((s, i) =>
            wb.getSnapshot().selection[0] === "stages" &&
            Number(wb.getSnapshot().selection[1]) === i ? (
              <StageEditor
                key={text(object(s).id)}
                index={i}
                recipes={recipes}
              />
            ) : null,
          )}
        </>
      ) : (
        <ol aria-label="Quest stages" className="space-y-3">
          {all.map((row, i) => {
            const stage = object(row),
              id = text(stage.id),
              name = stageName(stage, q),
              open = opened === id;
            return (
              <li
                key={id || i}
                className={`rounded-xl border ${open ? "border-accent" : "border-line"} bg-panel/40`}
                onDragOver={(e) => e.preventDefault()}
                onDrop={(e) => {
                  e.preventDefault();
                  const source = e.dataTransfer.getData("text/plain");
                  if (!/^\d+$/.test(source)) return;
                  const from = Number(source);
                  if (from === i || from >= all.length) return;
                  wb.edit((value) => {
                    const rows = array(object(value).stages);
                    rows.splice(i, 0, rows.splice(from, 1)[0]!);
                  });
                }}
              >
                <div className="flex items-center gap-3 p-3">
                  <span
                    draggable
                    onDragStart={(e) =>
                      e.dataTransfer.setData("text/plain", String(i))
                    }
                    aria-label={`Drag ${name}`}
                    className="cursor-grab px-2 text-muted"
                  >
                    ⠿
                  </span>
                  <span className="flex size-9 shrink-0 items-center justify-center rounded-full border border-line text-muted">
                    {i + 1}
                  </span>
                  <button
                    type="button"
                    className="flex flex-1 items-center justify-between gap-4 p-2 text-left"
                    aria-label={name}
                    aria-expanded={open}
                    aria-controls={`stage-${i}`}
                    onClick={() => setOpened(open ? "" : id)}
                  >
                    <span>
                      <strong className="block text-base">{name}</strong>
                      {!open && (
                        <small className="text-muted">
                          {text(
                            stage.description,
                            stageConfigs[text(stage.type)]?.description,
                          )}
                        </small>
                      )}
                    </span>
                    <span>{open ? "⌃" : "⌄"}</span>
                  </button>
                  <Action
                    size="sm"
                    aria-label={`Move ${name} up`}
                    disabled={i === 0}
                    onClick={() => move(["stages"], i, -1)}
                  >
                    ↑
                  </Action>
                  <Action
                    size="sm"
                    aria-label={`Move ${name} down`}
                    disabled={i === all.length - 1}
                    onClick={() => move(["stages"], i, 1)}
                  >
                    ↓
                  </Action>
                </div>
                {open && (
                  <div
                    id={`stage-${i}`}
                    className="space-y-5 border-t border-line p-5 sm:ml-14"
                  >
                    <StageEditor index={i} recipes={recipes} />
                  </div>
                )}
              </li>
            );
          })}
        </ol>
      )}
      {!all.length && (
        <p className="py-12 text-center text-muted">
          Add the first stage to start your quest.
        </p>
      )}
      <Panel title="Advanced quest settings">
        <div className="flex flex-wrap gap-3">
          <Action onClick={() => wb.advanced("fields")}>
            All quest fields
          </Action>
          <Action onClick={() => wb.advanced("journal")}>
            Journal & objectives
          </Action>
          <Action onClick={() => wb.advanced("contacts")}>
            Contacts & messages
          </Action>
        </div>
      </Panel>
    </div>
  );
}
export async function editCopy(): Promise<void> {
  const data = object(wb.getSnapshot().doc?.value),
    result = await ask({
      title: "Title and description",
      fields: [
        { name: "title", label: "Title", value: nameOf(data) },
        {
          name: "description",
          label: "Description",
          value: text(data.description),
          type: "textarea",
        },
      ],
      confirm: "Save",
    });
  if (!result) return;
  if (!result.title?.trim()) throw Error("Enter a title.");
  wb.edit((value) => {
    Object.assign(object(value), {
      title: result.title!.trim(),
      description: result.description || "",
    });
  });
}
function applyMeeting(
  stage: JsonObject,
  recipe: JsonObject,
  recipes: JsonObject[],
): void {
  const previous = recipes.find((s) => s.community === stage.community);
  for (const key of [
    "contact",
    "community",
    "scene",
    "appearance",
    "scene_origin",
    "setup_trigger",
    "rearm_trigger",
    "accept_exit",
    "defer_exit",
    "scene_entry",
    "resume_entry",
    "activity",
    "community_phase",
    "defer_policy",
    "allow_defer",
    "opening_message",
  ]) {
    if (recipe[key] !== undefined) stage[key] = clone(recipe[key]);
    else if (!["contact", "community", "scene"].includes(key))
      delete stage[key];
  }
  if (recipe.phase_prefabs)
    stage.phase_prefabs = [
      ...new Set([
        ...array(stage.phase_prefabs).filter(
          (p) => !array(previous?.phase_prefabs).includes(p),
        ),
        ...array(recipe.phase_prefabs),
      ]),
    ];
  if (recipe.required_assets)
    stage.required_assets = [
      ...new Set([
        ...array(stage.required_assets),
        ...array(recipe.required_assets),
      ]),
    ];
}
function StageEditor({
  index,
  recipes,
}: {
  index: number;
  recipes: JsonObject[];
}) {
  const { doc, catalog } = useWorkbench(),
    q = object(doc?.value),
    stage = object(array(q.stages)[index]),
    path: Path = ["stages", index];
  const config = stageConfigs[text(stage.type)],
    contract = catalog.blocks.find((b) => b.type === stage.type),
    objectiveId = text(stage.objective).match(
      /^@objectives\.([^.]+)\.path$/,
    )?.[1];
  const objective = objectiveId
    ? object(object(q.composition).objectives)[objectiveId]
    : undefined;
  const copyPath = (key: string): Path =>
    objectiveId && objective !== undefined
      ? ["composition", "objectives", objectiveId, key]
      : [...path, key === "text" ? "title" : key];
  const copy = (key: string, value: string): void => {
    if (typeof objective === "string" && objectiveId)
      wb.set(["composition", "objectives", objectiveId], {
        text: objective,
        [key]: value,
      });
    else wb.set(copyPath(key), value);
  };
  const routes = object(stage.on),
    outcomes = Object.keys(routes).length
      ? routes
      : { success: object(array(q.stages)[index + 1]).id || "$end" };
  const options = [
    { value: "$end", label: "End quest" },
    ...array(q.stages).map((s) => ({
      value: text(object(s).id),
      label: stageName(object(s), q),
    })),
  ];
  return (
    <div className="space-y-5">
      <label className="grid gap-2 text-sm">
        Stage title
        <Input
          value={stageName(stage, q)}
          onChange={(e) => copy("text", e.target.value)}
        />
      </label>
      <label className="grid gap-2 text-sm">
        Stage description
        <textarea
          className="min-h-24 rounded border border-line bg-background p-3"
          value={text(object(objective).description, text(stage.description))}
          onChange={(e) => copy("description", e.target.value)}
        />
      </label>
      {objective !== undefined && (
        <p className="text-xs text-muted">
          Edits the linked journal objective. Stages using that objective share
          this text.
        </p>
      )}
      {stage.type === "meet_contact" && recipes.length > 0 && (
        <label className="grid gap-2 text-sm">
          Meeting binding
          <Select
            value={JSON.stringify([
              stage.contact,
              stage.community,
              stage.scene,
            ])}
            onChange={(e) => {
              const recipe = recipes.find(
                (r) =>
                  JSON.stringify([r.contact, r.community, r.scene]) ===
                  e.target.value,
              );
              if (recipe)
                wb.edit((value) =>
                  applyMeeting(object(at(value, path)), recipe, recipes),
                );
            }}
          >
            <option value="">Choose a reviewed meeting</option>
            {recipes.map((r) => {
              const key = JSON.stringify([r.contact, r.community, r.scene]);
              return (
                <option key={key} value={key}>
                  {text(r.contact)} · {text(r.community)}
                </option>
              );
            })}
          </Select>
        </label>
      )}
      <div className="grid gap-5 sm:grid-cols-2">
        {(config?.fields || Object.keys(contract?.fields || {}))
          .filter((k) => k in stage || contract?.fields[k])
          .map((key) => {
            const choices = referenceChoices(key);
            if (stage.type === "meet_contact" && key === "community")
              return (
                <label key={key} className="grid gap-2 text-sm">
                  Location
                  <Select
                    value={text(stage.community)}
                    onChange={(e) =>
                      wb.edit((value) => {
                        const target = object(at(value, path)),
                          recipe = recipes.find(
                            (r) => r.community === e.target.value,
                          );
                        if (recipe) applyMeeting(target, recipe, recipes);
                        else {
                          target.community = e.target.value;
                          delete target.scene_origin;
                          delete target.setup_trigger;
                          delete target.rearm_trigger;
                        }
                      })
                    }
                  >
                    <option value="">Choose location</option>
                    {!choices.some((o) => o.value === stage.community) && (
                      <option value={text(stage.community)}>
                        {text(stage.community)}
                      </option>
                    )}
                    {choices.map((o) => (
                      <option key={o.value} value={o.value}>
                        {o.label}
                      </option>
                    ))}
                  </Select>
                </label>
              );
            return (
              <Field
                key={key}
                path={[...path, key]}
                label={fieldLabels[key] || human(key)}
                {...(choices.length ? { options: choices } : {})}
                schema={contract?.fields[key]?.schema || {}}
              />
            );
          })}
      </div>
      {stage.type === "meet_contact" && (
        <div className="grid gap-4 sm:grid-cols-2">
          {[
            "scene_origin",
            "setup_trigger",
            ...(stage.defer_policy === "rearm" ? ["rearm_trigger"] : []),
          ].map((key) => (
            <Field
              key={key}
              path={[...path, key]}
              label={fieldLabels[key] || human(key)}
              options={referenceChoices(key)}
            />
          ))}
        </div>
      )}
      <div className="space-y-3">
        <h3 className="text-sm font-semibold">Outcomes</h3>
        {Object.entries(outcomes).map(([route, target]) =>
          typeof target === "string" ? (
            <label
              key={route}
              className="grid gap-2 text-sm sm:grid-cols-[180px_1fr] sm:items-center"
            >
              {route === "success"
                ? "When complete"
                : `If ${route.replaceAll("_", " ")}`}
              <Select
                value={target}
                onChange={(e) =>
                  wb.set([...path, "on"], {
                    ...object(stage.on),
                    [route]: e.target.value,
                  })
                }
              >
                {!options.some((o) => o.value === target) && (
                  <option value={target}>{target}</option>
                )}
                {options.map((o) => (
                  <option key={o.value} value={o.value}>
                    {o.label}
                  </option>
                ))}
              </Select>
            </label>
          ) : (
            <Field key={route} path={[...path, "on", route]} />
          ),
        )}
      </div>
      <Panel title="Advanced stage settings">
        <Fields path={path} />
        <Action
          variant="danger"
          onClick={async () => {
            if (
              await ask({
                title: "Delete this stage?",
                message:
                  "References to this stage may need updating. You can undo this change.",
                confirm: "Delete stage",
              })
            )
              wb.set(path, undefined);
          }}
        >
          Delete stage
        </Action>
      </Panel>
    </div>
  );
}
