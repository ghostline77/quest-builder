import { array, nameOf, object, text } from "./model";
import type { Json, JsonObject, Path } from "./model";
export interface Position {
  x: number;
  y: number;
  z: number;
  yaw: number;
}
export interface Placement extends Position {
  value: JsonObject;
  path: Path;
  type: string;
  moveYaw: number;
  unresolved?: boolean;
}
export function worldPlacements(data: JsonObject): {
  placements: Placement[];
  origin: Position;
  anchors: Map<string, Position>;
} {
  const placements: Placement[] = [],
    anchors = new Map<string, Position>();
  const raw = data.origin || { x: 0, y: 0, z: 0, yaw: data.yaw || 0 },
    base = Array.isArray(raw)
      ? { x: raw[0], y: raw[1], z: raw[2], yaw: data.yaw || 0 }
      : object(raw);
  const origin = {
    x: Number(base.x || 0),
    y: Number(base.y || 0),
    z: Number(base.z || 0),
    yaw: Number(base.yaw || 0),
  };
  anchors.set("origin", origin);
  const position = (raw: Json | undefined): Position | undefined => {
    if (Array.isArray(raw))
      return {
        x: Number(raw[0]),
        y: Number(raw[1]),
        z: Number(raw[2]),
        yaw: 0,
      };
    const v = object(raw);
    if (["x", "y", "z"].every((k) => k in v))
      return {
        x: Number(v.x),
        y: Number(v.y),
        z: Number(v.z),
        yaw: typeof v.yaw === "number" ? v.yaw : origin.yaw,
      };
    const anchor = anchors.get(text(v.from, "origin"));
    if (!anchor) return;
    const yaw =
        typeof v.yaw === "number"
          ? v.yaw
          : (anchors.get(text(v.yaw))?.yaw ?? anchor.yaw),
      angle = (yaw * Math.PI) / 180;
    const forward =
        Number(v.forward || 0) +
        Math.cos((Number(v.bearing || 0) * Math.PI) / 180) *
          Number(v.distance || 0),
      right =
        Number(v.right || 0) +
        Math.sin((Number(v.bearing || 0) * Math.PI) / 180) *
          Number(v.distance || 0);
    return {
      x: anchor.x + Math.cos(angle) * forward - Math.sin(angle) * right,
      y: anchor.y + Math.sin(angle) * forward + Math.cos(angle) * right,
      z: anchor.z + Number(v.up ?? v.z_offset ?? 0),
      yaw,
    };
  };
  const pending: { value: JsonObject; path: Path; type: string }[] = [];
  const queue = (value: Json | undefined, path: Path, type: string) =>
    pending.push({ value: object(value), path, type });
  for (const type of [
    "markers",
    "triggers",
    "entities",
    "loot_containers",
    "crowd_null_areas",
    "devices",
    "ai_spots",
    "splines",
    "patrol_splines",
  ])
    array(data[type]).forEach((v, i) => queue(v, [type, i], type));
  const communities = [
    ...(data.community
      ? [{ value: object(data.community), path: ["community"] as Path }]
      : []),
    ...array(data.communities).map((value, i) => ({
      value: object(value),
      path: ["communities", i] as Path,
    })),
  ];
  for (const { value: community, path } of communities) {
    const entries = community.entries
      ? array(community.entries).map((v, i) => ({
          entry: { ...community, ...object(v) },
          source: object(v),
          path: [...path, "entries", i],
        }))
      : [{ entry: community, source: community, path }];
    for (const e of entries) {
      const spots = e.source.spots || e.entry.spots;
      if (spots)
        array(spots).forEach((spot, i) =>
          queue(
            { ...object(community.spot), ...object(spot) },
            [...(e.source.spots ? e.path : path), "spots", i],
            "community",
          ),
        );
      else if (e.source.spot || community.spot)
        queue(
          { ...object(community.spot), ...object(e.source.spot) },
          [...(e.source.spot ? e.path : path), "spot"],
          "community",
        );
    }
    queue(community, path, "community area");
  }
  // Resolve forward anchor references as well as already-declared anchors.
  while (pending.length) {
    let resolved = false;
    for (let i = 0; i < pending.length;) {
      const entry = pending[i]!,
        p = position(entry.value.position);
      if (!p) {
        i++;
        continue;
      }
      const moveYaw = p.yaw;
      p.yaw =
        typeof entry.value.yaw === "number"
          ? entry.value.yaw
          : (anchors.get(text(entry.value.yaw))?.yaw ?? p.yaw);
      placements.push({ ...entry, ...p, moveYaw });
      const ref = text(entry.value.ref);
      if (ref)
        for (const alias of [
          ref,
          ref.replace(/^#/, ""),
          ref.startsWith("$/")
            ? ref
            : `${text(data.prefab_root)}/${ref.replace(/^#/, "")}`,
        ])
          anchors.set(alias, p);
      pending.splice(i, 1);
      resolved = true;
    }
    if (!resolved) break;
  }
  for (const entry of pending)
    placements.push({
      ...entry,
      x: 0,
      y: 0,
      z: 0,
      yaw: 0,
      moveYaw: 0,
      unresolved: true,
    });
  return { placements, origin, anchors };
}
export function movedPosition(
  placement: Placement,
  x: number,
  y: number,
  anchors: Map<string, Position>,
): JsonObject {
  const old = placement.value.position,
    anchor = anchors.get(text(object(old).from, "origin"));
  const round = (v: number) => Math.round(v * 1000) / 1000;
  if (
    old &&
    !Array.isArray(old) &&
    typeof old === "object" &&
    !("x" in old) &&
    anchor
  ) {
    const angle = (placement.moveYaw * Math.PI) / 180,
      dx = x - anchor.x,
      dy = y - anchor.y;
    const next: JsonObject = {
      ...old,
      forward: round(Math.cos(angle) * dx + Math.sin(angle) * dy),
      right: round(-Math.sin(angle) * dx + Math.cos(angle) * dy),
    };
    delete next.distance;
    delete next.bearing;
    return next;
  }
  return { x: round(x), y: round(y), z: placement.z };
}
export function outlinePoints(value: JsonObject): [number, number][] {
  const points = array(value.points);
  if (points.length)
    return points.map((p) =>
      Array.isArray(p)
        ? [Number(p[0]), Number(p[1])]
        : [Number(object(p).x), Number(object(p).y)],
    );
  if (["circle", "disc", "regular_polygon"].includes(text(value.type))) {
    const count = Math.max(
        3,
        Math.min(512, Number(value.points_count ?? value.segments ?? 12)),
      ),
      radius = Number(value.radius ?? 1);
    return Array.from({ length: count }, (_, i) => [
      Math.cos((i * Math.PI * 2) / count) * radius,
      Math.sin((i * Math.PI * 2) / count) * radius,
    ]);
  }
  const w = Number(value.width ?? value.size ?? 2) / 2,
    d = Number(value.depth ?? value.length ?? value.size ?? w * 2) / 2;
  return [
    [-w, -d],
    [w, -d],
    [w, d],
    [-w, d],
  ];
}
export const placementName = (v: JsonObject): string => nameOf(v);
