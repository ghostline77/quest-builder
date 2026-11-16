import { useId, useState } from "react";
import type { ReactNode } from "react";
import { Button, Input, Select } from "@ghostline/ui-core";
import { ask } from "./dialogs";
import {
  array,
  at,
  human,
  isRaw,
  nameOf,
  object,
  parse,
  pointer,
  pretty,
  references,
  text,
  write,
} from "./model";
import type { Json, Path, Schema } from "./model";
import { useWorkbench, workbench as wb } from "./store";
export function Action({
  children,
  onClick,
  ...props
}: Omit<React.ComponentProps<typeof Button>, "onClick"> & {
  onClick: () => void | Promise<unknown>;
}) {
  return (
    <Button type="button" {...props} onClick={() => wb.run(onClick)}>
      {children}
    </Button>
  );
}
export function Panel({
  title,
  children,
  open = false,
}: {
  title: string;
  children: ReactNode;
  open?: boolean;
}) {
  return (
    <details
      open={open}
      className="rounded-lg border border-line bg-panel/40 p-4"
    >
      <summary className="cursor-pointer text-sm font-semibold">
        {title}
      </summary>
      <div className="mt-4 space-y-4">{children}</div>
    </details>
  );
}
export function Field({
  path,
  label,
  options,
  schema: supplied,
}: {
  path: Path;
  label?: string;
  options?: { value: string; label: string }[];
  schema?: Schema;
}) {
  const { doc } = useWorkbench(),
    id = useId();
  const value = at(doc?.value, path),
    schema = wb.resolve(supplied || wb.schema(path));
  const [invalid, setInvalid] = useState(false);
  const name = label || human(String(path.at(-1) || "Value"));
  const choices =
    options ||
    schema.enum?.map((v) => ({ value: pretty(v), label: String(v) }));
  if (value !== null && typeof value === "object" && !isRaw(value))
    return <Fields path={path} />;
  const numeric =
    typeof value === "number" ||
    isRaw(value) ||
    schema.type === "number" ||
    schema.type === "integer";
  return (
    <div className="grid min-w-0 gap-2 text-sm">
      <label htmlFor={id} className="font-medium text-foreground/80">
        {name}
      </label>
      {choices ? (
        <Select
          id={id}
          value={
            options ? text(value) : value === undefined ? "" : pretty(value)
          }
          onChange={(e) =>
            wb.set(path, options ? e.target.value : parse(e.target.value))
          }
        >
          <option value="">Choose {name.toLowerCase()}</option>
          {value !== undefined &&
            !choices.some(
              (o) => o.value === (options ? text(value) : pretty(value)),
            ) && (
              <option value={options ? text(value) : pretty(value)}>
                {text(value)}
              </option>
            )}
          {choices.map((o) => (
            <option key={o.value} value={o.value}>
              {o.label}
            </option>
          ))}
        </Select>
      ) : typeof value === "boolean" || schema.type === "boolean" ? (
        <input
          id={id}
          type="checkbox"
          className="size-5 accent-accent"
          checked={value === true}
          onChange={(e) => wb.set(path, e.target.checked)}
        />
      ) : value === null ? (
        <Select
          id={id}
          value="null"
          onChange={(e) =>
            wb.set(
              path,
              (
                {
                  string: "",
                  number: 0,
                  boolean: false,
                  object: {},
                  array: [],
                } as Record<string, Json>
              )[e.target.value] ?? null,
            )
          }
        >
          {["null", "string", "number", "boolean", "object", "array"].map(
            (t) => (
              <option key={t}>{t}</option>
            ),
          )}
        </Select>
      ) : /text|description|caption|note/i.test(String(path.at(-1))) ||
        text(value).length > 100 ? (
        <textarea
          id={id}
          className="min-h-24 w-full rounded border border-line bg-background px-3 py-2"
          value={text(value)}
          onChange={(e) => wb.set(path, e.target.value)}
        />
      ) : numeric ? (
        <NumericInput
          key={pointer(path)}
          id={id}
          value={value}
          invalid={invalid}
          onInvalid={setInvalid}
          onChange={(v) => wb.set(path, v)}
        />
      ) : (
        <Input
          id={id}
          value={text(value)}
          onChange={(e) => wb.set(path, e.target.value)}
        />
      )}
      {invalid && (
        <small className="text-red-300">Enter a valid JSON number.</small>
      )}
      {schema.description && (
        <small className="text-muted">{schema.description}</small>
      )}
      {typeof value === "string" &&
        /^(?:@|#)|\.(scene|ent|app|mesh|wem|workspot)$/.test(value) && (
          <Action onClick={() => wb.advanced("references", path)}>
            Find reference
          </Action>
        )}
      {typeof value === "string" &&
        ["id", "key"].includes(String(path.at(-1))) && (
          <Action
            onClick={async () => {
              const result = await ask({
                title: "Rename authoring reference",
                message:
                  "Updates exact named references in this document. Review other files through References.",
                fields: [{ name: "name", label: "New name", value }],
                confirm: "Rename",
              });
              if (!result?.name?.trim() || result.name === value) return;
              wb.edit((data) => {
                for (const ref of references(data))
                  if (ref.value === value)
                    write(data, ref.path, result.name!.trim());
              });
            }}
          >
            Rename references…
          </Action>
        )}
    </div>
  );
}
function NumericInput({
  id,
  value,
  invalid,
  onInvalid,
  onChange,
}: {
  id: string;
  value: Json | undefined;
  invalid: boolean;
  onInvalid: (v: boolean) => void;
  onChange: (v: Json) => void;
}) {
  const source = isRaw(value) ? text(object(value).rawJSON) : text(value);
  const [draft, setDraft] = useState(source),
    [last, setLast] = useState(source);
  if (source !== last) {
    setLast(source);
    setDraft(source);
  }
  return (
    <Input
      id={id}
      inputMode="decimal"
      aria-invalid={invalid}
      value={draft}
      onChange={(e) => {
        setDraft(e.target.value);
        try {
          const next = parse(e.target.value);
          if (typeof next !== "number" && !isRaw(next)) throw Error("number");
          onInvalid(false);
          onChange(next);
        } catch {
          onInvalid(true);
        }
      }}
    />
  );
}
export function Fields({
  path = [],
  depth = 0,
}: {
  path?: Path;
  depth?: number;
}) {
  const { doc } = useWorkbench();
  const value = at(doc?.value, path),
    schema = wb.schema(path);
  if (value === undefined)
    return (
      <div className="flex flex-wrap items-center gap-3 text-muted">
        <span>{human(String(path.at(-1) || "Value"))} is not configured.</span>
        <Action
          onClick={() =>
            wb.set(path, wb.default(schema, String(path.at(-1) || "")))
          }
        >
          Configure
        </Action>
      </div>
    );
  if (value === null || typeof value !== "object" || isRaw(value))
    return <Field path={path} />;
  const entries = Object.entries(value);
  return (
    <div className="space-y-4">
      {entries.map(([key, entry]) => {
        const child = [...path, Array.isArray(value) ? Number(key) : key],
          complex =
            entry !== null && typeof entry === "object" && !isRaw(entry);
        return (
          <div key={key} className="min-w-0 rounded border border-line/70 p-3">
            <div className="mb-2 flex flex-wrap items-center justify-between gap-2">
              <span className="text-xs text-muted">
                {Array.isArray(value)
                  ? `${Number(key) + 1} · ${nameOf(entry)}`
                  : human(key)}
              </span>
              <div className="flex gap-1">
                {Array.isArray(value) && (
                  <>
                    <Action
                      aria-label={`Move ${nameOf(entry)} up`}
                      disabled={Number(key) === 0}
                      onClick={() => move(path, Number(key), -1)}
                    >
                      ↑
                    </Action>
                    <Action
                      aria-label={`Move ${nameOf(entry)} down`}
                      disabled={Number(key) === value.length - 1}
                      onClick={() => move(path, Number(key), 1)}
                    >
                      ↓
                    </Action>
                  </>
                )}
                <Action
                  aria-label={`Remove ${key}`}
                  onClick={async () => {
                    if (
                      await ask({
                        title: `Remove ${Array.isArray(value) ? nameOf(entry) : human(key)}?`,
                        message: "You can undo this change.",
                        confirm: "Remove",
                      })
                    )
                      wb.set(child, undefined);
                  }}
                >
                  Remove
                </Action>
              </div>
            </div>
            {complex ? (
              <Panel
                title={
                  human(key) +
                  ` · ${Array.isArray(entry) ? entry.length : Object.keys(entry).length} entries`
                }
                open={depth < 1}
              >
                <Fields path={child} depth={depth + 1} />
              </Panel>
            ) : (
              <Field path={child} />
            )}
          </div>
        );
      })}
      <Action onClick={() => addField(path, value, schema)}>
        {Array.isArray(value) ? "Add entry" : "Add field"}
      </Action>
    </div>
  );
}
export function move(path: Path, index: number, delta: number): void {
  wb.edit((data) => {
    const rows = array(at(data, path)),
      target = index + delta;
    if (target < 0 || target >= rows.length) return;
    [rows[index], rows[target]] = [rows[target]!, rows[index]!];
  });
}
async function addField(
  path: Path,
  value: Json[] | import("./model").JsonObject,
  schema: Schema,
): Promise<void> {
  if (Array.isArray(value)) {
    const key = String(path.at(-1)),
      n = value.length + 1;
    const defaults: Record<string, Json> = {
      actors: {
        key: `actor_${n}`,
        kind: "community",
        community_ref: "",
        entry: "",
      },
      sections: { key: `section_${n}`, lines: [], on_end: [] },
      choices: { key: `choice_${n}`, options: [] },
      options: { choice_key: "", caption: "", target: "" },
      spoken_lines: { key: `line_${n}`, speaker: "", text: "", duration_ms: 0 },
      choice_lines: { key: `choice_${n}`, text: "" },
      entry_points: { name: `entry_${n}`, node: "" },
      exit_points: { name: `exit_${n}`, node: "" },
      quest_nodes: {
        key: `command_${n}`,
        kind: "realtime_delay",
        duration_ms: 1000,
        on_out: [],
      },
      start_nodes: { key: `start_${n}`, on_start: [] },
      end_nodes: { key: `end_${n}` },
      markers: { ref: `#marker_${n}`, position: { from: "origin" } },
      triggers: {
        ref: `#trigger_${n}`,
        position: { from: "origin" },
        outline: { type: "circle", radius: 3, height: 3 },
        notifiers: ["quest"],
      },
      on_start: { node: "" },
      on_out: { node: "" },
      on_end: { node: "" },
      entities: {
        ref: `#entity_${n}`,
        entity: "",
        appearance: "default",
        position: { from: "origin" },
      },
      devices: {
        ref: `#device_${n}`,
        kind: "access_point",
        buffer_id:
          Math.max(0, ...value.map((v) => Number(object(v).buffer_id || 1))) +
          1,
        position: { from: "origin" },
      },
      loot_containers: {
        ref: `#loot_${n}`,
        entity_template:
          "base\\gameplay\\loot\\containers\\weapon_cases\\weapon_case_small.ent",
        appearance: "default",
        position: { from: "origin" },
        items: [{ id: "Items.Preset_Katana_Default", quantity: 1, chance: 1 }],
      },
    };
    const entry =
      schema.items && Object.keys(schema.items).length
        ? wb.default(schema.items)
        : (defaults[key] ?? (typeof value[0] === "object" ? {} : ""));
    wb.set([...path, value.length], entry);
    return;
  }
  const result = await ask({
    title: "Add field",
    fields: [
      { name: "name", label: "Field name" },
      {
        name: "type",
        label: "Value type",
        options: [
          "From contract",
          "String",
          "Number",
          "Boolean",
          "Object",
          "Array",
          "Null",
        ].map((v) => ({ value: v, label: v })),
      },
    ],
    message: Object.keys(schema.properties || {})
      .filter((k) => !Object.hasOwn(value, k))
      .join(", "),
    confirm: "Add",
  });
  if (!result?.name?.trim()) return;
  const key = result.name.trim();
  if (Object.hasOwn(value, key)) throw Error("This field already exists.");
  const next =
    result.type === "From contract"
      ? wb.default(schema.properties?.[key], key)
      : ((
          {
            String: "",
            Number: 0,
            Boolean: false,
            Object: {},
            Array: [],
            Null: null,
          } as Record<string, Json>
        )[result.type || "String"] ?? "");
  wb.set([...path, key], next);
}
export function Source() {
  const { doc } = useWorkbench();
  return (
    <div className="space-y-3">
      <p className="text-sm text-muted">
        Edits use the same document as the structured views. Invalid JSON stays
        unsaved until corrected.
      </p>
      <textarea
        aria-label="Document source"
        spellCheck={false}
        className="min-h-[65dvh] w-full rounded border border-line bg-background p-4 font-mono text-sm"
        aria-invalid={doc?.invalid}
        value={doc?.text || ""}
        onChange={(e) => wb.source(e.target.value)}
      />
      {doc?.invalid && <p className="text-red-300">Invalid JSON · not saved</p>}
    </div>
  );
}
