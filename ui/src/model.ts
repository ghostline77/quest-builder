export type Json = null | boolean | number | string | Json[] | JsonObject;
export interface JsonObject {
  [key: string]: Json | undefined;
}
export type Path = (string | number)[];
export const object = (value: Json | undefined): JsonObject =>
  value !== null && typeof value === "object" && !Array.isArray(value)
    ? value
    : {};
export const array = (value: Json | undefined): Json[] =>
  Array.isArray(value) ? value : [];
export const text = (value: Json | undefined, fallback = ""): string =>
  typeof value === "string"
    ? value
    : typeof value === "number"
      ? String(value)
      : fallback;
export const human = (value: string): string =>
  value.replaceAll("_", " ").replace(/\b\w/g, (c) => c.toUpperCase());
export const nameOf = (value: Json | undefined): string => {
  const v = object(value);
  return text(
    v.title || v.name || v.key || v.id || v.ref || v.type,
    "Untitled",
  );
};
export const pointer = (path: Path): string =>
  "/" +
  path
    .map((k) => String(k).replaceAll("~", "~0").replaceAll("/", "~1"))
    .join("/");
export const at = (value: Json | undefined, path: Path): Json | undefined =>
  path.reduce<Json | undefined>(
    (v, k) => (Array.isArray(v) ? v[Number(k)] : object(v)[String(k)]),
    value,
  );
export function write(value: Json, path: Path, next: Json | undefined): void {
  let parent: Json | undefined = value;
  for (let i = 0; i < path.length - 1; i++) {
    const key = path[i]!;
    if (!parent || typeof parent !== "object" || isRaw(parent))
      throw Error("The selected field no longer exists.");
    let child: Json | undefined = Array.isArray(parent)
      ? parent[Number(key)]
      : parent[String(key)];
    if (child === undefined) {
      if (next === undefined) return;
      child = typeof path[i + 1] === "number" ? [] : {};
      if (Array.isArray(parent)) parent[Number(key)] = child;
      else parent[String(key)] = child;
    }
    parent = child;
  }
  const key = path.at(-1);
  if (key === undefined || parent === null || typeof parent !== "object")
    throw Error("The selected field no longer exists.");
  if (Array.isArray(parent)) {
    if (next === undefined) parent.splice(Number(key), 1);
    else parent[Number(key)] = next;
  } else if (next === undefined) delete parent[String(key)];
  else parent[String(key)] = next;
}
interface LosslessJSON extends JSON {
  rawJSON?: (source: string) => JsonObject;
}
export function parse(source: string): Json {
  return JSON.parse(source, ((
    _: string,
    value: Json,
    context?: { source?: string },
  ) => {
    if (
      typeof value === "number" &&
      Number.isInteger(value) &&
      !Number.isSafeInteger(value)
    ) {
      const raw = (JSON as LosslessJSON).rawJSON;
      if (!raw || !context?.source)
        throw Error(
          "This browser cannot safely edit 64-bit numeric IDs. Use a current browser.",
        );
      return raw(context.source);
    }
    return value;
  }) as Parameters<typeof JSON.parse>[1]) as Json;
}
export const pretty = (value: Json): string => JSON.stringify(value, null, 2);
export const clone = <T extends Json>(value: T): T => parse(pretty(value)) as T;
export const isRaw = (value: Json | undefined): boolean =>
  Object.hasOwn(object(value), "rawJSON");
export function kind(path: string, value?: Json): string {
  const v = object(value);
  if (v.stages || (v.schema_version && v.id && v.title)) return "quest";
  if (path.endsWith(".world.json") || v.quest_sector_path || v.prefab_root)
    return "world";
  if (path.includes("scene-spec") || (v.sections && v.actors)) return "scene";
  if (v.spoken_lines || v.choice_lines) return "dialogue";
  if (path.includes("package")) return "package";
  if (path.includes("characters/") || path.endsWith(".character.json"))
    return "character";
  return "document";
}
export function relative(base: string, path: string): string {
  const parts = base.replaceAll("\\", "/").split("/").slice(0, -1);
  for (const part of path.replaceAll("\\", "/").split("/")) {
    if (part === "..") parts.pop();
    else if (part && part !== ".") parts.push(part);
  }
  return parts.join("/");
}
export interface Schema {
  $defs?: Record<string, Schema>;
  $ref?: string;
  type?: string | string[];
  properties?: Record<string, Schema>;
  additionalProperties?: boolean | Schema;
  required?: string[];
  items?: Schema;
  allOf?: Schema[];
  oneOf?: Schema[];
  anyOf?: Schema[];
  enum?: Json[];
  const?: Json;
  default?: Json;
  minimum?: number;
  maximum?: number;
  minLength?: number;
  description?: string;
}
export interface Block {
  type: string;
  fields: Record<string, { required: boolean; schema: Schema }>;
  outcomes?: string[];
}
export interface Catalog {
  blocks: Block[];
  definitions: Record<string, Schema>;
}
export interface Argument {
  id: string;
  long: string | null;
  positional: boolean;
  required: boolean;
  help: string | null;
  flag: boolean;
  multiple: boolean;
  defaults: string[];
}
export interface Command {
  name: string;
  about: string | null;
  args: Argument[];
}
export interface Root {
  id: number;
  name: string;
  path: string;
}
export interface Bootstrap {
  token: string;
  roots: Root[];
  commands: Command[];
  schema: Schema;
}
export interface FileGroup {
  id: number;
  files: { path: string; size: number }[];
}
export interface DiskFile {
  text: string;
  hash: string;
}
export interface AuthoringDocument {
  root: number;
  path: string;
  text: string;
  hash: string | null;
  value?: Json;
  media?: string;
  dirty: boolean;
  invalid: boolean;
  revision: number;
}
export interface RecordFile {
  path: string;
  value: Json;
  type: string;
}
export interface Job {
  id: string;
  command: string;
  args: string[];
  status: string;
  stdout: string;
  stderr: string;
  error?: string;
}
export interface Diagnostic {
  path?: string;
  code?: string;
  message: string;
  root?: number;
  file?: string;
}
export interface Settings {
  game: string;
  schema: string;
  characterBuilderUrl: string;
}
export function resolveSchema(
  schema: Schema = {},
  root: Schema = {},
  definitions: Record<string, Schema> = {},
  depth = 0,
): Schema {
  if (depth > 20) return {};
  if (schema.$ref) {
    const keys = schema.$ref.replace(/^#\//, "").split("/");
    let found: unknown = root;
    for (const key of keys)
      found =
        found && typeof found === "object"
          ? (found as Record<string, unknown>)[key]
          : undefined;
    return resolveSchema(
      (found || definitions[keys.at(-1) || ""] || {}) as Schema,
      root,
      definitions,
      depth + 1,
    );
  }
  if (schema.allOf) {
    const parts = schema.allOf.map((s) =>
      resolveSchema(s, root, definitions, depth + 1),
    );
    return {
      ...schema,
      properties: Object.assign({}, ...parts.map((s) => s.properties)),
      required: [...new Set(parts.flatMap((s) => s.required || []))],
    };
  }
  return schema;
}
export function defaultValue(
  schema: Schema = {},
  key = "",
  root: Schema = {},
  definitions: Record<string, Schema> = {},
  depth = 0,
): Json {
  if (depth > 20) return null;
  const s = resolveSchema(schema, root, definitions);
  if (s.default !== undefined) return clone(s.default);
  if (s.const !== undefined) return clone(s.const);
  if (s.enum?.length) return clone(s.enum[0]!);
  const alternative = (s.oneOf || s.anyOf)?.[0];
  if (alternative)
    return defaultValue(alternative, key, root, definitions, depth + 1);
  if (s.type === "object" || s.properties)
    return Object.fromEntries(
      (s.required || []).map((k) => [
        k,
        defaultValue(s.properties?.[k], k, root, definitions, depth + 1),
      ]),
    );
  if (s.type === "array" || s.items) return [];
  if (s.type === "boolean") return false;
  if (s.type === "number" || s.type === "integer") return s.minimum || 0;
  if (
    [
      "on",
      "composition",
      "resources",
      "position",
      "origin",
      "outline",
    ].includes(key)
  )
    return {};
  if (
    [
      "stages",
      "actors",
      "sections",
      "choices",
      "spoken_lines",
      "choice_lines",
      "markers",
      "triggers",
      "entities",
      "devices",
      "communities",
      "quest_nodes",
      "start_nodes",
      "end_nodes",
      "entry_points",
      "exit_points",
      "lines",
      "options",
      "on_start",
      "on_out",
      "on_end",
      "grants",
      "performance",
      "appearances",
      "loadout",
    ].includes(key)
  )
    return [];
  return "";
}
export function references(
  value: Json | undefined,
  path: Path = [],
): { path: Path; value: string }[] {
  if (typeof value === "string") return [{ path, value }];
  if (!value || typeof value !== "object" || isRaw(value)) return [];
  return Object.entries(value).flatMap(([key, entry]) =>
    references(entry, [...path, Array.isArray(value) ? Number(key) : key]),
  );
}
