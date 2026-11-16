import { useEffect, useState } from "react";
import { Input, Select } from "@ghostline/ui-core";
import { client } from "./api";
import { ask } from "./dialogs";
import { Action, Fields, Source } from "./fields";
import { at, human, kind, object, pointer, references, text } from "./model";
import type { Command, Diagnostic, Path } from "./model";
import { useWorkbench, workbench as wb } from "./store";
import { FlowGraph, PlacementMap, Timeline } from "./graphs";
export function Advanced() {
  const { doc, view, selection } = useWorkbench();
  if (!doc)
    return <p className="text-muted">Open a project file to edit it.</p>;
  if (doc.media)
    return /\.(png|jpe?g|webp)$/i.test(doc.path) ? (
      <img
        className="max-h-[70dvh] max-w-full"
        src={doc.media}
        alt={doc.path}
      />
    ) : /\.(mp4|webm)$/i.test(doc.path) ? (
      <video className="max-h-[70dvh] max-w-full" src={doc.media} controls />
    ) : (
      <audio src={doc.media} controls />
    );
  const type = kind(doc.path, doc.value),
    views = [
      "overview",
      ...(type === "quest"
        ? ["flow", "journal", "contacts"]
        : type === "scene"
          ? ["graph", "timeline", "actors"]
          : type === "world"
            ? ["placement"]
            : []),
      "fields",
      "references",
      "source",
    ];
  const target: Path =
    view === "journal"
      ? ["composition", "objectives"]
      : view === "contacts"
        ? ["composition", "contacts"]
        : view === "actors"
          ? ["actors"]
          : selection;
  return (
    <div className="space-y-5">
      <nav aria-label="Editor views" className="flex flex-wrap gap-2">
        {views.map((v) => (
          <Action
            key={v}
            aria-pressed={v === view}
            variant={view === v ? "primary" : "ghost"}
            onClick={() => wb.update({ view: v })}
          >
            {human(v)}
          </Action>
        ))}
      </nav>
      {view === "source" || doc.value === undefined ? (
        <Source />
      ) : view === "flow" ? (
        <FlowGraph />
      ) : view === "graph" ? (
        <FlowGraph scene />
      ) : view === "placement" ? (
        <PlacementMap key={doc.path} />
      ) : view === "timeline" ? (
        <Timeline />
      ) : view === "references" ? (
        <References />
      ) : view === "overview" ? (
        <div className="space-y-4">
          <p className="text-muted">
            {text(object(doc.value).description, "Authoring document")}
          </p>
          <div className="flex flex-wrap gap-2">
            {Object.entries(object(doc.value)).map(([key, value]) => (
              <Action
                key={key}
                onClick={() => wb.update({ view: "fields", selection: [key] })}
              >
                {human(key)} ·{" "}
                {Array.isArray(value) ? value.length : typeof value}
              </Action>
            ))}
          </div>
        </div>
      ) : (
        <>
          <div className="flex items-center gap-3">
            <code className="text-muted">{pointer(target)}</code>
            <Action
              onClick={() => wb.update({ selection: [], view: "fields" })}
            >
              Document properties
            </Action>
          </div>
          <Fields path={target} />
        </>
      )}
    </div>
  );
}
function References() {
  const { doc, root, files, selection } = useWorkbench(),
    [query, setQuery] = useState(text(at(doc?.value, selection))),
    [matches, setMatches] = useState<string[]>([]),
    [loading, setLoading] = useState(false);
  useEffect(() => {
    let cancelled = false;
    const timer = setTimeout(() => {
      if (!query) {
        setMatches([]);
        return;
      }
      setLoading(true);
      wb.run(async () => {
        const found: string[] = [];
        try {
          for (const file of files.find((f) => f.id === root)?.files || []) {
            if (cancelled) return;
            if (
              file.path === doc?.path ||
              !file.path.endsWith(".json") ||
              file.size > 300000
            )
              continue;
            const source = await client.request("read", {
              root,
              path: file.path,
            });
            if (source.text.includes(query)) found.push(file.path);
          }
          if (!cancelled) setMatches(found);
        } finally {
          if (!cancelled) setLoading(false);
        }
      });
    }, 350);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [query, root, files, doc?.path]);
  const local = query
    ? references(doc?.value).filter((r) => r.value.includes(query))
    : [];
  return (
    <div className="space-y-4">
      <Input
        type="search"
        aria-label="Find references"
        placeholder="Find a named reference, resource path or fact…"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
      />
      <p aria-live="polite" className="text-muted">
        {local.length} matches in this document · {matches.length} other files{" "}
        {loading ? "· searching…" : ""}
      </p>
      {local.map((r) => (
        <div
          key={pointer(r.path)}
          className="flex flex-wrap items-center gap-3 rounded border border-line p-3"
        >
          <Action
            onClick={() => wb.update({ view: "fields", selection: r.path })}
          >
            {pointer(r.path)}
          </Action>
          <code className="break-all text-xs">{r.value}</code>
        </div>
      ))}
      {matches.map((path) => (
        <Action key={path} onClick={() => wb.open(path)}>
          {path}
        </Action>
      ))}
    </div>
  );
}
export async function projectSettings(): Promise<boolean> {
  const settings = wb.getSnapshot().settings;
  const result = await ask({
    title: "Project settings",
    message: "Local tool paths are saved for this project.",
    fields: [
      { name: "game", label: "Game directory", value: settings.game },
      {
        name: "schema",
        label: "Reflection override (optional)",
        value: settings.schema,
      },
      {
        name: "characterBuilderUrl",
        label: "Character-builder URL",
        value: settings.characterBuilderUrl,
      },
    ],
    confirm: "Save settings",
  });
  if (!result) return false;
  wb.settings({
    game: result.game?.trim() || "",
    schema: result.schema?.trim() || "",
    characterBuilderUrl: result.characterBuilderUrl?.trim() || "",
  });
  return true;
}
export async function build(): Promise<void> {
  if (wb.getSnapshot().settings.game) {
    await wb.build();
    return;
  }
  const result = await ask({
    title: "Set up resource generation",
    message:
      "Choose a game directory for native generation, or generate inspection documents without it.",
    fields: [
      {
        name: "mode",
        label: "Build output",
        options: [
          { value: "setup", label: "Set up native generation" },
          { value: "documents", label: "Generate resource documents" },
        ],
      },
    ],
  });
  if (!result) return;
  if (result.mode === "documents") await wb.build(true);
  else if ((await projectSettings()) && wb.getSnapshot().settings.game)
    await wb.build();
}
export async function compareDisk(): Promise<void> {
  const doc = wb.getSnapshot().doc;
  if (!doc) return;
  const disk = await client.request("read", { root: doc.root, path: doc.path });
  const result = await ask({
    title: "Resolve external changes",
    content: (
      <div className="grid gap-4">
        <label className="grid gap-2">
          Editor version
          <textarea
            className="h-40 rounded border border-line bg-background p-3 font-mono text-xs"
            value={doc.text}
            readOnly
          />
        </label>
        <label className="grid gap-2">
          Version on disk
          <textarea
            className="h-40 rounded border border-line bg-background p-3 font-mono text-xs"
            value={disk.text}
            readOnly
          />
        </label>
      </div>
    ),
    fields: [
      {
        name: "mode",
        label: "Resolution",
        options: [
          {
            value: "mine",
            label:
              "Keep editor version and save over the reviewed disk version",
          },
          { value: "disk", label: "Use disk version" },
        ],
      },
    ],
    confirm: "Apply choice",
  });
  if (result) await wb.acceptDisk(disk, result.mode === "mine");
}
export async function reload(): Promise<void> {
  if (!wb.getSnapshot().doc) return;
  if (
    wb.getSnapshot().doc?.dirty &&
    !(await ask({
      title: "Reload from disk?",
      message:
        "Unsaved edits will be discarded. Saved versions remain in work/editor-history.",
      confirm: "Reload",
    }))
  )
    return;
  await wb.reload();
}
export async function history(): Promise<void> {
  const doc = wb.getSnapshot().doc;
  if (!doc) return;
  const history = await client.request("history", {
    root: doc.root,
    path: doc.path,
  });
  if (!history.versions.length) {
    wb.notify("No earlier saved versions for this document.");
    return;
  }
  const result = await ask({
    title: "Restore a saved version",
    message:
      "Restoring creates an editable change. The current saved version remains in history.",
    fields: [
      {
        name: "version",
        label: "Version",
        options: history.versions.map((v) => ({
          value: v,
          label: new Date(Number(BigInt(v) / 1000000n)).toLocaleString(),
        })),
      },
    ],
    confirm: "Restore",
  });
  if (!result?.version) return;
  const saved = await client.request("history", {
    root: doc.root,
    path: doc.path,
    version: result.version,
  });
  if (saved.text === undefined) throw Error("History version has no source.");
  wb.source(saved.text);
}
export async function newFile(): Promise<void> {
  const result = await ask({
    title: "Create authoring document",
    fields: [
      {
        name: "path",
        label: "Workspace-relative path",
        value: "quests/new_quest/document.json",
      },
      { name: "source", label: "Source", value: "{}", type: "textarea" },
    ],
    confirm: "Create",
  });
  if (!result?.path?.trim()) return;
  const path = result.path.trim(),
    source = result.source || "";
  if (path.endsWith(".json")) JSON.parse(source);
  await wb.flush();
  await client.request("save", {
    root: wb.getSnapshot().root,
    path,
    text: source + "\n",
    hash: null,
  });
  await wb.refresh();
  await wb.open(path, true);
  wb.advanced("source");
}
export function inferArgs(command: Command): Record<string, string | boolean> {
  const { doc, settings } = wb.getSnapshot(),
    result: Record<string, string | boolean> = {};
  for (const arg of command.args) {
    let value: string | boolean = arg.flag ? false : arg.defaults.join("\n");
    if (doc) {
      if (arg.id === "out")
        value = `work/builds/${text(object(doc.value).id || object(doc.value).name, "quest")}-${new Date().toISOString().replace(/[:.]/g, "-")}`;
      if (["spec", "manifest", "profile", "resource"].includes(arg.id))
        value = doc.path;
      if (["root", "project"].includes(arg.id)) value = ".";
      if (arg.id === "name") value = text(object(doc.value).id);
    }
    if (arg.id === "game") value = settings.game;
    if (arg.id === "schema") value = settings.schema;
    result[arg.id] = value;
  }
  return result;
}
export function commandArgs(
  command: Command,
  values: Record<string, string | boolean>,
): string[] {
  const args: string[] = [];
  // clap's positionals must retain their catalogue order.
  for (const arg of command.args) {
    const value = values[arg.id];
    if (arg.flag) {
      if (value && arg.long) args.push("--" + arg.long);
    } else if (value !== undefined && String(value).trim()) {
      if (arg.long) args.push("--" + arg.long);
      args.push(
        ...(arg.multiple
          ? String(value)
              .split("\n")
              .map((s) => s.trim())
              .filter(Boolean)
          : [String(value)]),
      );
    } else if (arg.required) throw Error(`${human(arg.id)} is required.`);
  }
  return args;
}
export function Activity() {
  const { bottom, jobs, diagnostics } = useWorkbench(),
    [expanded, setExpanded] = useState(false),
    [selected, setSelected] = useState("");
  if (!bottom) return null;
  return (
    <section
      aria-label="Activity and compiler tools"
      className="shrink-0 border-t border-line bg-panel"
    >
      <div className="flex flex-wrap items-center gap-2 px-4 py-3">
        {(["activity", "diagnostics", "tools"] as const).map((t) => (
          <Action
            key={t}
            variant={bottom === t ? "primary" : "ghost"}
            onClick={() => wb.update({ bottom: t })}
          >
            {t === "activity"
              ? `Activity · ${jobs.filter((j) => j.status === "running").length}`
              : t === "diagnostics"
                ? `Issues · ${diagnostics.length}`
                : "Compiler tools"}
          </Action>
        ))}
        <Action className="ml-auto" onClick={() => setExpanded(!expanded)}>
          {expanded ? "Collapse" : "Expand"}
        </Action>
        <Action
          aria-label="Close activity panel"
          onClick={() => wb.update({ bottom: null })}
        >
          Close
        </Action>
      </div>
      <div
        className={`${expanded ? "max-h-[65dvh]" : "max-h-[32dvh]"} overflow-auto p-4`}
      >
        {bottom === "tools" ? (
          <Tools />
        ) : bottom === "diagnostics" ? (
          <div className="space-y-3">
            {!diagnostics.length && (
              <p className="text-muted">
                No diagnostics recorded. Check the current document to validate
                it.
              </p>
            )}
            {diagnostics.map((d, i) => (
              <Action
                key={i}
                className="flex w-full justify-start whitespace-normal text-left"
                onClick={() => reveal(d)}
              >
                <code>{d.path || d.code}</code>
                <span>{d.message}</span>
              </Action>
            ))}
          </div>
        ) : (
          <div className="space-y-3">
            {!jobs.length && (
              <p className="text-muted">
                Build and inspection results will appear here.
              </p>
            )}
            {[...jobs].reverse().map((j) => (
              <div key={j.id} className="rounded border border-line p-3">
                <Action
                  className="flex w-full justify-start"
                  onClick={() => setSelected(selected === j.id ? "" : j.id)}
                >
                  <span
                    className={
                      j.status === "failed"
                        ? "text-red-300"
                        : j.status === "succeeded"
                          ? "text-accent"
                          : "text-muted"
                    }
                  >
                    {j.status}
                  </span>
                  <strong>{human(j.command)}</strong>
                  <code className="overflow-hidden text-ellipsis text-xs text-muted">
                    {j.args.join(" ")}
                  </code>
                </Action>
                {(selected === j.id || j.status === "failed") && (
                  <div className="mt-4 space-y-3">
                    <pre className="overflow-auto whitespace-pre-wrap break-all rounded bg-background p-3 text-xs">
                      {[j.stdout, j.stderr, j.error]
                        .filter(Boolean)
                        .join("\n") || "Waiting for output…"}
                    </pre>
                    <div className="flex gap-2">
                      <Action
                        onClick={async () => {
                          const command = wb
                            .getSnapshot()
                            .bootstrap?.commands.find(
                              (c) => c.name === j.command,
                            );
                          if (!command) return;
                          const values = inferArgs(command);
                          let pos = 0;
                          for (let i = 0; i < j.args.length; i++) {
                            const arg = j.args[i]!;
                            const spec = arg.startsWith("--")
                              ? command.args.find(
                                  (a) => a.long === arg.slice(2),
                                )
                              : command.args.filter((a) => a.positional)[pos++];
                            if (spec)
                              values[spec.id] = spec.flag
                                ? true
                                : arg.startsWith("--")
                                  ? j.args[++i] || ""
                                  : arg;
                          }
                          setToolPreset({ command: command.name, values });
                          wb.update({ bottom: "tools" });
                        }}
                      >
                        Repeat with options
                      </Action>
                      {j.status === "running" && (
                        <Action
                          variant="danger"
                          onClick={() =>
                            client.request("jobs/cancel", { id: j.id })
                          }
                        >
                          Cancel job
                        </Action>
                      )}
                    </div>
                  </div>
                )}
              </div>
            ))}
          </div>
        )}
      </div>
    </section>
  );
}
async function reveal(issue: Diagnostic): Promise<void> {
  if (issue.root !== undefined && issue.root !== wb.getSnapshot().root)
    await wb.switchRoot(issue.root);
  if (issue.file && issue.file !== wb.getSnapshot().doc?.path)
    await wb.open(issue.file);
  const path =
    issue.path
      ?.replace(/^\//, "")
      .split("/")
      .filter(Boolean)
      .map((s) => s.replaceAll("~1", "/").replaceAll("~0", "~")) || [];
  wb.advanced("fields", path);
}
let toolPreset:
  { command: string; values: Record<string, string | boolean> } | undefined;
function setToolPreset(preset: NonNullable<typeof toolPreset>) {
  toolPreset = preset;
}
function Tools() {
  const { bootstrap } = useWorkbench(),
    commands = bootstrap?.commands || [];
  const [name, setName] = useState(toolPreset?.command || "compile"),
    [values, setValues] = useState<Record<string, string | boolean>>(
      () =>
        toolPreset?.values ||
        (commands.length
          ? inferArgs(
              commands.find((c) => c.name === "compile") || commands[0]!,
            )
          : {}),
    );
  const command = commands.find((c) => c.name === name) || commands[0];
  if (!command) return <p>No compiler commands available.</p>;
  return (
    <form
      className="space-y-4"
      onSubmit={(e) => {
        e.preventDefault();
        wb.run(async () => {
          const args = commandArgs(command, values);
          if (
            ["install", "rollback", "replace-build"].includes(command.name) &&
            !(await ask({
              title: human(command.name),
              message:
                "Review the exact operation before changing the installation or published build.",
              content: (
                <pre className="whitespace-pre-wrap break-all rounded bg-background p-3 text-xs">
                  {command.name + "\n" + args.join("\n")}
                </pre>
              ),
              confirm: "Run operation",
            }))
          )
            return;
          await wb.start(command.name, args);
        });
      }}
    >
      <Select
        aria-label="Compiler command"
        value={command.name}
        onChange={(e) => {
          setName(e.target.value);
          const next = commands.find((c) => c.name === e.target.value);
          if (next) setValues(inferArgs(next));
          toolPreset = undefined;
        }}
      >
        {commands.map((c) => (
          <option key={c.name} value={c.name}>
            {human(c.name)}
          </option>
        ))}
      </Select>
      <p className="text-muted">{command.about}</p>
      <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-3">
        {command.args.map((arg) => (
          <label key={arg.id} className="grid gap-2 text-sm">
            {human(arg.id)}
            {arg.required ? " *" : ""}
            {arg.flag ? (
              <input
                type="checkbox"
                className="size-5 accent-accent"
                checked={values[arg.id] === true}
                onChange={(e) =>
                  setValues({ ...values, [arg.id]: e.target.checked })
                }
              />
            ) : arg.multiple ? (
              <textarea
                className="min-h-20 rounded border border-line bg-background p-2"
                value={String(values[arg.id] || "")}
                onChange={(e) =>
                  setValues({ ...values, [arg.id]: e.target.value })
                }
              />
            ) : (
              <Input
                value={String(values[arg.id] || "")}
                onChange={(e) =>
                  setValues({ ...values, [arg.id]: e.target.value })
                }
              />
            )}
            <small className="text-muted">{arg.help}</small>
          </label>
        ))}
      </div>
      <button
        type="submit"
        className="rounded bg-accent px-4 py-2 font-semibold text-accent-ink"
      >
        Run {human(command.name)}
      </button>
    </form>
  );
}
