import { useSyncExternalStore } from "react";
import { client, saveNew } from "./api";
import {
  array,
  at,
  clone,
  defaultValue,
  kind,
  object,
  parse,
  pretty,
  relative,
  resolveSchema,
  text,
  write,
} from "./model";
import type {
  AuthoringDocument,
  Bootstrap,
  Catalog,
  Diagnostic,
  FileGroup,
  Job,
  Json,
  JsonObject,
  Path,
  RecordFile,
  Schema,
  Settings,
} from "./model";
export type Section = "story" | "scenes" | "locations" | "cast";
export interface WorkbenchState {
  bootstrap: Bootstrap | null;
  catalog: Catalog;
  root: number;
  files: FileGroup[];
  records: RecordFile[];
  indexErrors: string[];
  doc: AuthoringDocument | null;
  questPath: string;
  section: Section;
  advanced: boolean;
  view: string;
  selection: Path;
  history: string[];
  future: string[];
  saving: boolean;
  conflict: boolean;
  loading: boolean;
  jobs: Job[];
  diagnostics: Diagnostic[];
  bottom: "activity" | "diagnostics" | "tools" | null;
  settings: Settings;
  notice: { message: string; bad: boolean } | null;
}
const emptySettings = (): Settings => ({
  game: "",
  schema: "",
  characterBuilderUrl: "",
});
export class Workbench {
  private state: WorkbenchState = {
    bootstrap: null,
    catalog: { blocks: [], definitions: {} },
    root: 0,
    files: [],
    records: [],
    indexErrors: [],
    doc: null,
    questPath: "",
    section: "story",
    advanced: false,
    view: "fields",
    selection: [],
    history: [],
    future: [],
    saving: false,
    conflict: false,
    loading: true,
    jobs: [],
    diagnostics: [],
    bottom: null,
    settings: emptySettings(),
    notice: null,
  };
  private listeners = new Set<() => void>();
  private saveTimer: ReturnType<typeof setTimeout> | undefined;
  private noticeTimer: ReturnType<typeof setTimeout> | undefined;
  private cache = new Map<string, AuthoringDocument>();
  private saving: Promise<void> | null = null;
  private jobSources = new Map<string, { root: number; file: string }>();
  private rootRequest = 0;
  private fileRequest = 0;
  private disposed = false;
  getSnapshot = (): WorkbenchState => this.state;
  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };
  update = (patch: Partial<WorkbenchState>): void => {
    if (this.disposed) return;
    this.state = { ...this.state, ...patch };
    this.listeners.forEach((fn) => fn());
  };
  report = (error: unknown): void => {
    console.error(error);
    this.notify(error instanceof Error ? error.message : String(error), true);
  };
  run = (fn: () => void | Promise<unknown>): void => {
    Promise.resolve().then(fn).catch(this.report);
  };
  notify(message: string, bad = false): void {
    clearTimeout(this.noticeTimer);
    this.update({ notice: { message, bad } });
    this.noticeTimer = setTimeout(
      () => this.update({ notice: null }),
      bad ? 11000 : 4000,
    );
  }
  async boot(): Promise<void> {
    const bootstrap = await client.request("bootstrap");
    client.token = bootstrap.token;
    const catalog = await client.request("catalog");
    this.update({ bootstrap, catalog });
    await this.switchRoot(bootstrap.roots[0]?.id ?? 0);
    try {
      const last = JSON.parse(
        localStorage.getItem("quest-workbench-last") || "null",
      ) as { root: string; path: string } | null;
      const root = bootstrap.roots.find((r) => r.path === last?.root);
      if (
        root &&
        last &&
        this.state.files
          .find((f) => f.id === root.id)
          ?.files.some((f) => f.path === last.path)
      ) {
        if (root.id !== this.state.root) await this.switchRoot(root.id);
        await this.open(last.path);
        return;
      }
    } catch (error) {
      this.report(error);
    }
    if (this.state.questPath) await this.open(this.state.questPath);
  }
  async refresh(): Promise<void> {
    this.update({ files: await client.request("files") });
    await this.index(this.state.root);
  }
  private async index(root: number): Promise<void> {
    const request = ++this.rootRequest;
    const files = this.state.files.find((f) => f.id === root)?.files || [];
    const candidates = files.filter(
      (f) =>
        f.path.endsWith(".json") &&
        f.size < 2_000_000 &&
        /\/quest\.json$|\.quest\.json$|scene-spec\.json$|\.world\.json$|\.character\.json$|_manifest\.json$|\.dialogue\.json$/.test(
          f.path,
        ),
    );
    const results = await Promise.allSettled(
      candidates.map(async (f) => {
        const cached = this.cache.get(`${root}:${f.path}`);
        const source = cached?.dirty
          ? cached.text
          : (await client.request("read", { root, path: f.path })).text;
        const value = parse(source);
        return { path: f.path, value, type: kind(f.path, value) };
      }),
    );
    if (request !== this.rootRequest || root !== this.state.root) return;
    const records = results.flatMap((r) =>
      r.status === "fulfilled" ? [r.value] : [],
    );
    const remembered = localStorage.getItem(
      `quest-active:${this.state.bootstrap?.roots.find((r) => r.id === root)?.path}`,
    );
    const quests = records.filter((r) => r.type === "quest");
    const questPath =
      quests.find((r) => r.path === this.state.questPath)?.path ||
      quests.find((r) => r.path === remembered)?.path ||
      quests[0]?.path ||
      "";
    this.update({
      records,
      questPath,
      indexErrors: results.flatMap((r, i) =>
        r.status === "rejected"
          ? [`${candidates[i]?.path}: ${String(r.reason)}`]
          : [],
      ),
      loading: false,
    });
  }
  async switchRoot(root: number): Promise<void> {
    await this.flush();
    ++this.fileRequest;
    const rootPath = this.state.bootstrap?.roots.find(
      (r) => r.id === root,
    )?.path;
    let settings = emptySettings();
    try {
      settings = {
        ...settings,
        ...(JSON.parse(
          localStorage.getItem(`quest-project-settings:${rootPath}`) ||
            localStorage.getItem("quest-workbench-settings") ||
            "{}",
        ) as Settings),
      };
    } catch {
      /* Use defaults for malformed saved preferences. */
    }
    this.update({
      root,
      doc: null,
      questPath: "",
      records: [],
      history: [],
      future: [],
      conflict: false,
      selection: [],
      settings,
      loading: true,
    });
    await this.refresh();
  }
  get quest(): RecordFile | undefined {
    return this.records.find((r) => r.path === this.state.questPath);
  }
  get records(): RecordFile[] {
    return this.state.records.map((r) =>
      this.state.doc?.path === r.path
        ? { ...r, value: this.state.doc.value ?? r.value }
        : r,
    );
  }
  documents(type: string, scoped = true): RecordFile[] {
    const q = this.quest,
      folder = q?.path.split("/").slice(0, -1).join("/");
    const resources = object(object(q?.value).resources);
    const paths = [
      ...array(resources.scenes),
      ...array(resources.worlds),
    ].flatMap((r) => {
      const spec = typeof r === "string" ? r : text(object(r).spec);
      return spec && q ? [relative(q.path, spec)] : [];
    });
    return this.records.filter(
      (r) =>
        r.type === type &&
        (!scoped ||
          !folder ||
          r.path.startsWith(folder + "/") ||
          paths.includes(r.path)),
    );
  }
  async open(path: string, force = false): Promise<void> {
    await this.flush();
    const root = this.state.root,
      request = ++this.fileRequest;
    let doc = !force ? this.cache.get(`${root}:${path}`) : undefined;
    if (!doc) {
      if (/\.(png|jpe?g|webp|wav|ogg|mp3|mp4|webm)$/i.test(path))
        doc = {
          root,
          path,
          text: "",
          hash: null,
          media: await client.media(root, path),
          dirty: false,
          invalid: false,
          revision: 0,
        };
      else {
        const disk = await client.request("read", { root, path });
        let value: Json | undefined;
        if (path.endsWith(".json")) {
          try {
            value = parse(disk.text);
          } catch (error) {
            this.report(error);
          }
        }
        doc = {
          ...disk,
          root,
          path,
          ...(value === undefined ? {} : { value }),
          dirty: false,
          invalid: false,
          revision: 0,
        };
      }
    }
    if (request !== this.fileRequest || root !== this.state.root) {
      if (doc.media) URL.revokeObjectURL(doc.media);
      return;
    }
    const old = this.cache.get(`${root}:${path}`);
    if (old?.media && old !== doc) URL.revokeObjectURL(old.media);
    this.cache.set(`${root}:${path}`, doc);
    const type = kind(path, doc.value),
      owner =
        type === "quest"
          ? path
          : this.documents("quest", false).find((q) =>
              path.startsWith(q.path.split("/").slice(0, -1).join("/") + "/"),
            )?.path || this.state.questPath;
    const section: Section =
      type === "scene" || type === "dialogue"
        ? "scenes"
        : type === "world"
          ? "locations"
          : type === "character"
            ? "cast"
            : this.state.section;
    this.update({
      doc,
      questPath: owner,
      section: type === "quest" ? "story" : section,
      history: [],
      future: [],
      conflict: false,
      selection: [],
      view:
        doc.value === undefined
          ? "source"
          : {
              quest: "flow",
              scene: "graph",
              world: "placement",
              dialogue: "dialogue",
            }[type] || "fields",
    });
    const rootPath = this.state.bootstrap?.roots.find(
      (r) => r.id === root,
    )?.path;
    localStorage.setItem(
      "quest-workbench-last",
      JSON.stringify({ root: rootPath, path }),
    );
    localStorage.setItem(`quest-active:${rootPath}`, owner);
  }
  async navigate(section: Section): Promise<void> {
    await this.flush();
    this.update({ section, advanced: false, selection: [] });
    if (
      section === "story" &&
      this.state.questPath &&
      this.state.doc?.path !== this.state.questPath
    )
      await this.open(this.state.questPath);
  }
  advanced(view = "fields", selection: Path = []): void {
    this.update({ advanced: true, view, selection });
  }
  edit(change: (value: Json) => Json | void): void {
    const doc = this.state.doc;
    if (!doc || doc.value === undefined || doc.invalid)
      throw Error("Open a valid authoring document first.");
    const value = clone(doc.value),
      result = change(value);
    this.commit({
      ...doc,
      value: result ?? value,
      text: pretty(result ?? value) + "\n",
      dirty: true,
      revision: doc.revision + 1,
    });
  }
  set(path: Path, next: Json | undefined): void {
    this.edit((value) => {
      if (!path.length) return next ?? null;
      write(value, path, next);
    });
  }
  source(source: string): void {
    const doc = this.state.doc;
    if (!doc) return;
    let value = doc.value,
      invalid = false;
    if (doc.path.endsWith(".json")) {
      try {
        value = parse(source);
      } catch {
        invalid = true;
      }
    }
    this.commit({
      ...doc,
      text: source,
      ...(value === undefined ? {} : { value }),
      invalid,
      dirty: true,
      revision: doc.revision + 1,
    });
  }
  private commit(
    doc: AuthoringDocument,
    history = [...this.state.history, this.state.doc!.text].slice(-100),
    future: string[] = [],
  ): void {
    this.cache.set(`${doc.root}:${doc.path}`, doc);
    this.update({ doc, history, future });
    this.scheduleSave();
  }
  private scheduleSave(): void {
    clearTimeout(this.saveTimer);
    if (!this.state.doc?.invalid)
      this.saveTimer = setTimeout(() => this.run(() => this.save()), 1300);
  }
  undo(redo = false): void {
    const doc = this.state.doc,
      source = [...(redo ? this.state.future : this.state.history)];
    if (!doc || !source.length) return;
    const next = source.pop()!;
    let value = doc.value,
      invalid = false;
    if (doc.path.endsWith(".json")) {
      try {
        value = parse(next);
      } catch {
        invalid = true;
      }
    }
    const other = [
      ...(redo ? this.state.history : this.state.future),
      doc.text,
    ];
    this.commit(
      {
        ...doc,
        text: next,
        ...(value === undefined ? {} : { value }),
        dirty: true,
        invalid,
        revision: doc.revision + 1,
      },
      redo ? other : source,
      redo ? source : other,
    );
  }
  async save(): Promise<void> {
    clearTimeout(this.saveTimer);
    if (this.saving) return this.saving;
    const doc = this.state.doc;
    if (!doc?.dirty || doc.invalid || this.state.conflict) return;
    this.update({ saving: true });
    this.saving = (async () => {
      try {
        const result = await client.request("save", {
          root: doc.root,
          path: doc.path,
          text: doc.text,
          hash: doc.hash,
        });
        const current = this.state.doc;
        if (current?.root === doc.root && current.path === doc.path) {
          const next = {
            ...current,
            hash: result.hash,
            dirty: current.revision !== doc.revision,
          };
          this.cache.set(`${doc.root}:${doc.path}`, next);
          this.update({
            doc: next,
            records: this.state.records.map((r) =>
              r.path === doc.path && next.value !== undefined
                ? { ...r, value: next.value }
                : r,
            ),
          });
        }
      } catch (error) {
        if (
          error instanceof Error &&
          /changed on disk|removed on disk/.test(error.message)
        )
          this.update({ conflict: true });
        throw error;
      } finally {
        this.saving = null;
        this.update({ saving: false });
        if (this.state.doc?.dirty && !this.state.conflict) this.scheduleSave();
      }
    })();
    return this.saving;
  }
  async flush(): Promise<void> {
    if (this.state.doc?.invalid)
      throw Error("Fix invalid JSON before continuing.");
    if (this.state.conflict)
      throw Error("Resolve the disk conflict before continuing.");
    if (this.saving) await this.saving;
    while (this.state.doc?.dirty) {
      await this.save();
      if (this.state.conflict)
        throw Error("Resolve the disk conflict before continuing.");
    }
  }
  async reload(): Promise<void> {
    const doc = this.state.doc;
    if (!doc) return;
    clearTimeout(this.saveTimer);
    if (this.saving) await this.saving;
    this.update({
      doc: { ...doc, dirty: false, invalid: false },
      conflict: false,
    });
    await this.open(doc.path, true);
  }
  async acceptDisk(
    disk: { text: string; hash: string },
    mine: boolean,
  ): Promise<void> {
    const doc = this.state.doc;
    if (!doc) return;
    if (mine) {
      this.update({
        doc: { ...doc, hash: disk.hash, dirty: true },
        conflict: false,
      });
      await this.save();
    } else await this.reload();
  }
  schema(path: Path): Schema {
    let schema =
      kind(this.state.doc?.path || "", this.state.doc?.value) === "quest"
        ? this.state.bootstrap?.schema || {}
        : {};
    for (let i = 0; i < path.length; i++) {
      schema = this.resolve(schema);
      if (path[0] === "stages" && i === 1) {
        const stage = object(at(this.state.doc?.value, path.slice(0, 2))),
          block = this.state.catalog.blocks.find((b) => b.type === stage.type);
        schema = {
          type: "object",
          properties: Object.fromEntries(
            Object.entries(block?.fields || {}).map(([k, f]) => [k, f.schema]),
          ),
          required: Object.entries(block?.fields || {})
            .filter(([, f]) => f.required)
            .map(([k]) => k),
        };
      } else
        schema = Array.isArray(at(this.state.doc?.value, path.slice(0, i)))
          ? schema.items || {}
          : schema.properties?.[String(path[i])] ||
            (typeof schema.additionalProperties === "object"
              ? schema.additionalProperties
              : {});
    }
    return this.resolve(schema);
  }
  resolve(schema: Schema): Schema {
    return resolveSchema(
      schema,
      this.state.bootstrap?.schema,
      this.state.catalog.definitions,
    );
  }
  default(schema: Schema = {}, key = ""): Json {
    return defaultValue(
      schema,
      key,
      this.state.bootstrap?.schema,
      this.state.catalog.definitions,
    );
  }
  settings(settings: Settings): void {
    if (
      settings.characterBuilderUrl &&
      !/^https?:\/\//.test(settings.characterBuilderUrl)
    )
      throw Error("Use an http or https character-builder URL.");
    localStorage.setItem(
      `quest-project-settings:${this.state.bootstrap?.roots.find((r) => r.id === this.state.root)?.path}`,
      JSON.stringify(settings),
    );
    this.update({ settings });
  }
  async start(command: string, args: string[]): Promise<void> {
    await this.flush();
    const doc = this.state.doc;
    const id = await client.start(this.state.root, command, args);
    if (doc) this.jobSources.set(id, { root: doc.root, file: doc.path });
    this.update({ bottom: "activity" });
    await this.poll();
    this.notify(`${command} started`);
  }
  async poll(): Promise<void> {
    const jobs = await client.request("jobs"),
      previous = new Map(this.state.jobs.map((j) => [j.id, j.status]));
    let diagnostics = [...this.state.diagnostics];
    for (const job of jobs)
      if (job.status !== "running" && previous.get(job.id) === "running") {
        let result: JsonObject = {};
        try {
          result = object(parse(job.stdout));
        } catch {
          /* Plain text tool output is displayed unchanged. */
        }
        const source = this.jobSources.get(job.id) || {};
        if (job.command === "validate")
          diagnostics = array(result.diagnostics).map((d) => ({
            ...source,
            message: text(object(d).message, pretty(d)),
            ...(typeof object(d).path === "string"
              ? { path: text(object(d).path) }
              : {}),
            ...(typeof object(d).code === "string"
              ? { code: text(object(d).code) }
              : {}),
          }));
        if (job.status === "failed" && !array(result.diagnostics).length)
          diagnostics.push({
            ...source,
            code: job.command,
            message: job.stderr || job.error || "Command failed",
          });
        if (result.valid) this.notify("Quest validation passed");
      }
    this.update({ jobs, diagnostics });
  }
  async validate(): Promise<void> {
    const doc = this.state.doc;
    if (!doc) return;
    const command = (
      {
        quest: "validate",
        scene: "scene-audit",
        world: "world-plan",
        package: "package",
      } as Record<string, string>
    )[kind(doc.path, doc.value)];
    if (command)
      await this.start(
        command,
        command === "package" ? [doc.path, "--plan"] : [doc.path],
      );
    else {
      await this.flush();
      this.notify(
        "Saved valid JSON. Use compiler tools for document-specific checks.",
      );
    }
  }
  async build(documentsOnly = false): Promise<void> {
    const doc = this.state.doc;
    if (!doc) return;
    const command = (
      { quest: "compile", scene: "scene", world: "world" } as Record<
        string,
        string
      >
    )[kind(doc.path, doc.value)];
    if (!command) throw Error("Choose a quest, scene or location to build.");
    const output = `work/builds/${text(object(doc.value).id || object(doc.value).name, "quest")}-${new Date().toISOString().replace(/[:.]/g, "-")}`;
    const args = [doc.path, "--out", output];
    if (documentsOnly) args.push("--documents-only");
    else {
      if (!this.state.settings.game)
        throw Error("Set the game directory in Project settings.");
      args.push("--game", this.state.settings.game);
      if (this.state.settings.schema)
        args.push("--schema", this.state.settings.schema);
    }
    await this.start(command, args);
  }
  async create(type: "quest" | "scene" | "world", name: string): Promise<void> {
    await this.flush();
    const key = name
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "_")
      .replace(/^_+|_+$/g, "");
    if (!/^[a-z][a-z0-9_]*$/.test(key))
      throw Error("Start the name with a letter.");
    const q = this.quest,
      namespace = type === "quest" ? key : text(object(q?.value).id, key),
      base =
        type === "quest"
          ? `quests/${key}`
          : q?.path.split("/").slice(0, -1).join("/");
    if (!base) throw Error("Choose a quest first.");
    const path =
      type === "quest"
        ? `${base}/quest.json`
        : type === "scene"
          ? `${base}/scenes/${key}.scene-spec.json`
          : `${base}/world/${key}.world.json`;
    const dialogue = `${base}/script/${key}.dialogue.json`,
      existing =
        (await client.request("files")).find((r) => r.id === this.state.root)
          ?.files || [];
    if (
      [path, ...(type === "scene" ? [dialogue] : [])].some((p) =>
        existing.some((f) => f.path.toLowerCase() === p.toLowerCase()),
      )
    )
      throw Error("A document with that name already exists.");
    let value: JsonObject;
    if (type === "quest")
      value = {
        schema_version: 1,
        id: key,
        title: name,
        description: "",
        stages: [],
        composition: {},
      };
    else if (type === "world")
      value = {
        name: namespace,
        title: name,
        description: "",
        prefab_root: `$/mod/${namespace}/#${key}`,
        block_path: `mod\\${namespace}\\world\\${key}.streamingblock`,
        quest_sector_path: `mod\\${namespace}\\world\\${key}.streamingsector`,
        origin: { x: 0, y: 0, z: 0, yaw: 0 },
        markers: [],
        triggers: [],
        entities: [],
      };
    else {
      value = {
        name: key,
        title: name,
        description: "",
        archive_path: `mod\\${namespace}\\scenes\\${key}.scene`,
        manifest: `../script/${key}.dialogue.json`,
        actors: [{ key: "v", kind: "player", name: "V" }],
        entry_point: { name: "start", node: "start" },
        exit_points: [{ name: "end", node: "end" }],
        start_node: { key: "start", on_start: [{ node: "end" }] },
        end_node: { key: "end" },
        sections: [],
        choices: [],
      };
      await saveNew(this.state.root, dialogue, {
        sequence_id: key,
        spoken_lines: [],
        choice_lines: [],
      });
    }
    await saveNew(this.state.root, path, value);
    if (type !== "quest" && q) {
      const disk = await client.request("read", {
          root: this.state.root,
          path: q.path,
        }),
        data = object(parse(disk.text));
      const resources = object(data.resources);
      data.resources = resources;
      const category = type === "scene" ? "scenes" : "worlds",
        entries = array(resources[category]);
      resources[category] = entries;
      entries.push(
        type === "scene"
          ? {
              spec: `scenes/${key}.scene-spec.json`,
              dialogue: `script/${key}.dialogue.json`,
            }
          : `world/${key}.world.json`,
      );
      await client.request("save", {
        root: this.state.root,
        path: q.path,
        hash: disk.hash,
        text: pretty(data) + "\n",
      });
      this.cache.delete(`${this.state.root}:${q.path}`);
    }
    await this.refresh();
    await this.open(path, true);
  }
  dispose(): void {
    this.disposed = true;
    clearTimeout(this.saveTimer);
    clearTimeout(this.noticeTimer);
    for (const doc of this.cache.values())
      if (doc.media) URL.revokeObjectURL(doc.media);
    this.listeners.clear();
  }
}
export const workbench = new Workbench();
export const useWorkbench = (): WorkbenchState =>
  useSyncExternalStore(workbench.subscribe, workbench.getSnapshot);
