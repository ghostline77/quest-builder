import { useRef, useState } from "react";
import { Action, Fields } from "./fields";
import { array, at, nameOf, object, pointer, text } from "./model";
import type { Json, Path } from "./model";
import { useWorkbench, workbench as wb } from "./store";
import { stageName } from "./story";
import { movedPosition, outlinePoints, worldPlacements } from "./world";
import type { Placement } from "./world";
export function FlowGraph({
  scene = false,
  inspect = true,
}: {
  scene?: boolean;
  inspect?: boolean;
}) {
  const { doc, selection } = useWorkbench(),
    data = object(doc?.value);
  const nodes: {
    path: Path;
    value: Json;
    id: string;
    label: string;
    targets: string[];
  }[] = [];
  if (!scene)
    array(data.stages).forEach((s, i, all) => {
      const v = object(s),
        on = object(v.on);
      nodes.push({
        path: ["stages", i],
        value: s,
        id: text(v.id),
        label: stageName(v, data),
        targets: Object.keys(on).length
          ? Object.values(on).flatMap((t) => (typeof t === "string" ? [t] : []))
          : [text(object(all[i + 1]).id, "$end")],
      });
    });
  else {
    const add = (value: Json, path: Path) => {
      const v = object(value),
        targets = ["on_start", "on_out", "on_end"].flatMap((k) =>
          array(v[k]).map((t) =>
            typeof t === "string" ? t : text(object(t).node),
          ),
        );
      targets.push(...array(v.options).map((o) => text(object(o).target)));
      nodes.push({ path, value, id: text(v.key), label: nameOf(v), targets });
    };
    for (const key of ["start_node", "end_node"])
      if (data[key]) add(data[key]!, [key]);
    for (const key of [
      "start_nodes",
      "sections",
      "choices",
      "quest_nodes",
      "xor_nodes",
      "end_nodes",
    ])
      array(data[key]).forEach((v, i) => add(v, [key, i]));
  }
  const columns = Math.max(1, Math.ceil(Math.sqrt(nodes.length))),
    rows = Math.ceil(nodes.length / columns),
    width = columns * 260 + 40,
    height = Math.max(200, rows * 150 + 40);
  const position = (i: number) => ({
    x: 30 + (i % columns) * 260,
    y: 30 + Math.floor(i / columns) * 150,
  });
  return (
    <div className="space-y-4">
      <div className="overflow-auto rounded-lg border border-line bg-background">
        <svg
          aria-label={scene ? "Scene graph" : "Quest graph"}
          role="img"
          width={width}
          height={height}
          className="min-w-full"
        >
          <defs>
            <marker
              id={scene ? "scene-arrow" : "quest-arrow"}
              viewBox="0 0 10 10"
              refX="9"
              refY="5"
              markerWidth="6"
              markerHeight="6"
              orient="auto-start-reverse"
            >
              <path d="M0 0 10 5 0 10z" fill="#c6ee83" />
            </marker>
          </defs>
          {nodes.flatMap((node, i) =>
            node.targets.map((target, j) => {
              const index = nodes.findIndex((n) => n.id === target);
              if (index < 0) return null;
              const a = position(i),
                b = position(index);
              return (
                <path
                  key={`${i}-${j}`}
                  d={`M${a.x + 210},${a.y + 35} C${a.x + 250},${a.y + 35} ${b.x - 35},${b.y + 35} ${b.x},${b.y + 35}`}
                  fill="none"
                  stroke="#85957e"
                  markerEnd={`url(#${scene ? "scene-arrow" : "quest-arrow"})`}
                />
              );
            }),
          )}
          {nodes.map((node, i) => {
            const p = position(i),
              selected = pointer(selection) === pointer(node.path);
            return (
              <g
                key={`${pointer(node.path)}-${node.id}`}
                transform={`translate(${p.x},${p.y})`}
                role="button"
                tabIndex={0}
                aria-label={node.label}
                onClick={() => wb.update({ selection: node.path })}
                onKeyDown={(e) => {
                  if (e.key === "Enter" || e.key === " ") {
                    e.preventDefault();
                    wb.update({ selection: node.path });
                  }
                }}
                className="cursor-pointer outline-none focus:stroke-accent"
              >
                <rect
                  width="210"
                  height="75"
                  rx="8"
                  fill="#202620"
                  stroke={selected ? "#c6ee83" : "#53604e"}
                  strokeWidth={selected ? 2 : 1}
                />
                <text x="14" y="28" fill="#edf3e8" fontSize="13">
                  {node.label.length > 27
                    ? node.label.slice(0, 25) + "…"
                    : node.label}
                </text>
                <text x="14" y="52" fill="#a2af9a" fontSize="10">
                  {node.id}
                </text>
                <title>
                  {node.label} → {node.targets.join(", ")}
                </title>
              </g>
            );
          })}
        </svg>
      </div>
      {!nodes.length && (
        <p className="text-muted">No nodes yet. Add entries in All fields.</p>
      )}
      {inspect &&
        selection.length > 0 &&
        at(doc?.value, selection) !== undefined && <Fields path={selection} />}
    </div>
  );
}
interface Drag {
  pointer: number;
  startX: number;
  startY: number;
  panX: number;
  panY: number;
  placement?: Placement;
}
export function PlacementMap() {
  const { doc, selection } = useWorkbench(),
    data = object(doc?.value),
    world = worldPlacements(data),
    resolved = world.placements.filter((p) => !p.unresolved);
  const fit = Math.min(
    20,
    340 / Math.max(15, ...resolved.map((p) => Math.abs(p.x - world.origin.x))),
    170 / Math.max(15, ...resolved.map((p) => Math.abs(p.y - world.origin.y))),
  );
  const [scale, setScale] = useState<number | null>(null),
    [pan, setPan] = useState({ x: 0, y: 0 }),
    [preview, setPreview] = useState<{
      path: string;
      dx: number;
      dy: number;
    } | null>(null);
  const drag = useRef<Drag | null>(null),
    zoom = scale ?? fit;
  return (
    <div className="space-y-4">
      <p className="text-sm text-muted">
        Drag placements to move them; drag the background to pan. Check terrain
        and collisions in game.
      </p>
      <div className="flex flex-wrap gap-2">
        <Action
          onClick={() => {
            setScale(null);
            setPan({ x: 0, y: 0 });
          }}
        >
          Fit
        </Action>
        <Action aria-label="Zoom in" onClick={() => setScale(zoom * 1.2)}>
          +
        </Action>
        <Action aria-label="Zoom out" onClick={() => setScale(zoom / 1.2)}>
          −
        </Action>
        <Action onClick={() => wb.update({ selection: ["origin"] })}>
          Origin / anchor
        </Action>
      </div>
      <svg
        viewBox="-400 -215 800 430"
        role="img"
        aria-label="Local quest placement editor"
        className="w-full touch-none rounded border border-line bg-background"
        onPointerDown={(e) => {
          e.currentTarget.setPointerCapture(e.pointerId);
          drag.current = {
            pointer: e.pointerId,
            startX: e.clientX,
            startY: e.clientY,
            panX: pan.x,
            panY: pan.y,
          };
        }}
        onPointerMove={(e) => {
          const d = drag.current;
          if (!d || d.pointer !== e.pointerId) return;
          const rect = e.currentTarget.getBoundingClientRect(),
            dx = ((e.clientX - d.startX) * 800) / rect.width,
            dy = ((e.clientY - d.startY) * 430) / rect.height;
          if (d.placement)
            setPreview({ path: pointer(d.placement.path), dx, dy });
          else setPan({ x: d.panX + dx, y: d.panY + dy });
        }}
        onPointerCancel={() => {
          drag.current = null;
          setPreview(null);
        }}
        onPointerUp={(e) => {
          const d = drag.current;
          drag.current = null;
          setPreview(null);
          if (!d?.placement) return;
          const p = d.placement,
            rect = e.currentTarget.getBoundingClientRect(),
            dx = ((e.clientX - d.startX) * 800) / rect.width,
            dy = ((e.clientY - d.startY) * 430) / rect.height;
          if (Math.abs(dx) + Math.abs(dy) > 2)
            wb.set(
              [...p.path, "position"],
              movedPosition(p, p.x + dx / zoom, p.y - dy / zoom, world.anchors),
            );
          wb.update({ selection: p.path });
        }}
      >
        <defs>
          <pattern
            id="map-grid"
            width={Math.max(10, 10 ** Math.floor(Math.log10(70 / zoom)) * zoom)}
            height={Math.max(
              10,
              10 ** Math.floor(Math.log10(70 / zoom)) * zoom,
            )}
            patternUnits="userSpaceOnUse"
            x={pan.x}
            y={pan.y}
          >
            <path
              d="M0 100 V0 H100"
              fill="none"
              stroke="#303a2e"
              strokeWidth=".5"
            />
          </pattern>
        </defs>
        <rect
          x="-400"
          y="-215"
          width="800"
          height="430"
          fill="url(#map-grid)"
        />
        <path
          d={`M${pan.x - 10},${pan.y} h20 M${pan.x},${pan.y - 10} v20`}
          stroke="#efb77f"
        />
        {resolved.map((p) => {
          const key = pointer(p.path),
            selected = key === pointer(selection),
            offset = preview?.path === key ? preview : { dx: 0, dy: 0 },
            outline =
              p.value.outline ||
              p.value.vehicle_forbidden_outline ||
              p.value.ambient_outline;
          return (
            <g
              key={key}
              transform={`translate(${(p.x - world.origin.x) * zoom + pan.x + offset.dx},${-(p.y - world.origin.y) * zoom + pan.y + offset.dy})`}
              role="button"
              tabIndex={0}
              aria-label={nameOf(p.value)}
              className="cursor-move"
              onKeyDown={(e) => {
                if (e.key === "Enter" || e.key === " ")
                  wb.update({ selection: p.path });
              }}
              onPointerDown={(e) => {
                e.stopPropagation();
                e.currentTarget.ownerSVGElement?.setPointerCapture(e.pointerId);
                drag.current = {
                  pointer: e.pointerId,
                  startX: e.clientX,
                  startY: e.clientY,
                  panX: pan.x,
                  panY: pan.y,
                  placement: p,
                };
              }}
            >
              {outline && (
                <polygon
                  points={outlinePoints(object(outline))
                    .map(([x, y]) => `${x * zoom},${-y * zoom}`)
                    .join(" ")}
                  transform={`rotate(${-p.yaw})`}
                  fill="#83b9e210"
                  stroke="#83b9e2"
                />
              )}
              <circle
                r={selected ? 7 : 5}
                fill={selected ? "#fff" : "#c6ee83"}
              />
              <path
                d={`M0,0 l${Math.cos((p.yaw * Math.PI) / 180) * 18},${-Math.sin((p.yaw * Math.PI) / 180) * 18}`}
                stroke="#c6ee83"
              />
              <title>{nameOf(p.value)}</title>
              {selected && (
                <text x="10" y="-10" fill="#fff" fontSize="11">
                  {nameOf(p.value)}
                </text>
              )}
            </g>
          );
        })}
      </svg>
      <p className="text-xs text-muted">
        Origin: {world.origin.x.toFixed(3)}, {world.origin.y.toFixed(3)},{" "}
        {world.origin.z.toFixed(3)} · {resolved.length} placements
      </p>
      {world.placements
        .filter((p) => p.unresolved)
        .map((p) => (
          <Action
            key={pointer(p.path)}
            variant="danger"
            onClick={() => wb.update({ selection: p.path })}
          >
            Unresolved anchor: {nameOf(p.value)}
          </Action>
        ))}
      {selection.length > 0 && <PanelSelection />}
    </div>
  );
}
function PanelSelection() {
  const { selection } = useWorkbench();
  return (
    <div className="rounded border border-line p-4">
      <Fields path={selection} />
    </div>
  );
}
export function Timeline() {
  const { doc } = useWorkbench(),
    data = object(doc?.value);
  const manifest =
    typeof data.manifest === "object"
      ? object(data.manifest)
      : object(
          wb.records.find(
            (r) =>
              r.path === importRelative(doc?.path || "", text(data.manifest)),
          )?.value,
        );
  let time = 0;
  return (
    <div className="space-y-3">
      <p className="text-muted">
        Authored timeline. Timing is verified in game.
      </p>
      {array(data.sections).flatMap((section) =>
        array(object(section).lines).map((line, i) => {
          const key = typeof line === "string" ? line : text(object(line).key),
            spec = object(
              array(manifest.spoken_lines).find((l) => object(l).key === key),
            );
          const duration = Number(spec.duration_ms || 0),
            start = time;
          time += duration;
          return (
            <div
              key={`${text(object(section).key)}-${i}`}
              className="grid grid-cols-[100px_1fr] gap-4 rounded border border-line p-3"
            >
              <span className="font-mono text-muted">
                {(start / 1000).toFixed(2)}s
              </span>
              <div>
                <strong>
                  {text(spec.speaker)} · {key}
                </strong>
                <p>{text(spec.text, "No dialogue wording")}</p>
                <small className="text-muted">
                  {duration ? `${duration} ms` : "Duration not authored"}
                </small>
              </div>
            </div>
          );
        }),
      )}
    </div>
  );
}
import { relative as importRelative } from "./model";
