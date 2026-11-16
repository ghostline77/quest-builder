import { test } from "node:test";
import assert from "node:assert/strict";
import { defaultValue, parse, pretty, relative } from "../src/model";
import { movedPosition, worldPlacements } from "../src/world";

test("refuses to round unsafe integers when runtime lacks lossless JSON", () => {
  if (!("rawJSON" in JSON))
    assert.throws(
      () => parse('{"id":18446744073709551615}'),
      /safely edit 64-bit/,
    );
  else
    assert.match(
      pretty(parse('{"id":18446744073709551615}')),
      /18446744073709551615/,
    );
});
test("schema required defaults resolve references", () => {
  const root = {
    $defs: {
      Delay: {
        type: "object",
        properties: { seconds: { type: "number", minimum: 3 } },
        required: ["seconds"],
      },
    },
  };
  assert.deepEqual(defaultValue({ $ref: "#/$defs/Delay" }, "", root), {
    seconds: 3,
  });
});
test("relative authoring paths resolve against scene folder", () =>
  assert.equal(
    relative(
      "quests/demo/scenes/intro.scene-spec.json",
      "../script/intro.dialogue.json",
    ),
    "quests/demo/script/intro.dialogue.json",
  ));
test("map resolves forward anchors and preserves height while moving polar offset", () => {
  const world = worldPlacements({
    origin: { x: 10, y: 20, z: 3, yaw: 90 },
    markers: [
      {
        ref: "#second",
        position: { from: "#first", distance: 5, bearing: 0, up: 2 },
      },
      { ref: "#first", position: { from: "origin", forward: 3 } },
    ],
  });
  const p = world.placements.find((p) => p.value.ref === "#second")!;
  assert.ok(p);
  assert.equal(p.unresolved, undefined);
  assert.equal(p.z, 5);
  assert.equal(p.y, 28);
  const next = movedPosition(p, 12, 30, world.anchors);
  assert.equal(next.distance, undefined);
  assert.equal(next.bearing, undefined);
  assert.equal(next.up, 2);
  assert.equal(next.forward, 7);
  assert.equal(next.right, -2);
});
test("map reports unknown and cyclic anchors instead of treating them as origin", () => {
  const world = worldPlacements({
    markers: [
      { ref: "#a", position: { from: "#b" } },
      { ref: "#b", position: { from: "#a" } },
    ],
  });
  assert.equal(world.placements.filter((p) => p.unresolved).length, 2);
});
