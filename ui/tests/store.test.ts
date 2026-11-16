import { afterEach, test } from "node:test";
import assert from "node:assert/strict";
import { Workbench } from "../src/store";
const instances: Workbench[] = [];
const originalFetch = globalThis.fetch;
afterEach(() => {
  instances.splice(0).forEach((w) => w.dispose());
  globalThis.fetch = originalFetch;
});
async function fixture() {
  const storage = new Map<string, string>();
  Object.defineProperty(globalThis, "localStorage", {
    configurable: true,
    value: {
      getItem: (k: string) => storage.get(k) ?? null,
      setItem: (k: string, v: string) => storage.set(k, v),
    },
  });
  let disk = {
    text: '{"id":"demo","title":"Demo","stages":[]}',
    hash: "initial",
  };
  const requests: { text: string; hash: string }[] = [];
  let saveGate: (() => Promise<void>) | undefined;
  globalThis.fetch = (async (
    input: string | URL | Request,
    init?: RequestInit,
  ) => {
    const endpoint = String(input).split("/").at(-1),
      body = init?.body
        ? (JSON.parse(String(init.body)) as { text: string; hash: string })
        : null;
    const json = (v: unknown, status = 200) =>
      new Response(JSON.stringify(v), {
        status,
        headers: { "content-type": "application/json" },
      });
    if (endpoint === "bootstrap")
      return json({
        token: "token",
        roots: [{ id: 0, name: "demo", path: "/demo" }],
        schema: {},
        commands: [],
      });
    if (endpoint === "catalog") return json({ blocks: [], definitions: {} });
    if (endpoint === "files")
      return json([
        { id: 0, files: [{ path: "quests/demo/quest.json", size: 100 }] },
      ]);
    if (endpoint === "read") return json(disk);
    if (endpoint === "save") {
      requests.push(body!);
      if (saveGate) await saveGate();
      if (body!.hash !== disk.hash)
        return json({ error: "file changed on disk" }, 400);
      disk = { text: body!.text, hash: "hash-" + requests.length };
      return json({ hash: disk.hash });
    }
    throw Error("Unexpected endpoint: " + endpoint);
  }) as typeof fetch;
  const wb = new Workbench();
  instances.push(wb);
  await wb.boot();
  return {
    wb,
    requests,
    disk: () => disk,
    external: () => {
      disk = { ...disk, hash: "external" };
    },
    gate: (fn: () => Promise<void>) => {
      saveGate = fn;
    },
  };
}
test("edits during an in-flight save retain dirty state and use the new disk hash", async () => {
  const f = await fixture();
  let release!: () => void;
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  let writes = 0;
  f.gate(async () => {
    if (++writes === 1) await gate;
  });
  f.wb.set(["title"], "First");
  const pending = f.wb.save();
  f.wb.set(["title"], "Second");
  release();
  await pending;
  assert.equal(f.wb.getSnapshot().doc?.dirty, true);
  await f.wb.flush();
  assert.equal(f.requests.length, 2);
  assert.equal(f.requests[1]!.hash, "hash-1");
  assert.equal(JSON.parse(f.disk().text).title, "Second");
  assert.equal(f.wb.getSnapshot().doc?.dirty, false);
});
test("disk conflicts block navigation and autosave until the reviewed version is accepted", async () => {
  const f = await fixture();
  f.wb.set(["title"], "Editor");
  f.external();
  await assert.rejects(f.wb.save(), /changed on disk/);
  assert.equal(f.wb.getSnapshot().conflict, true);
  await assert.rejects(f.wb.open("other.json"), /disk conflict/);
  assert.equal(f.requests.length, 1);
  await f.wb.acceptDisk(f.disk(), true);
  assert.equal(f.requests[1]!.hash, "external");
  assert.equal(f.wb.getSnapshot().conflict, false);
});
test("invalid source never saves and undo restores the valid document", async () => {
  const f = await fixture();
  f.wb.source("{");
  await f.wb.save();
  assert.equal(f.requests.length, 0);
  await assert.rejects(f.wb.flush(), /invalid JSON/);
  f.wb.undo();
  assert.equal(f.wb.getSnapshot().doc?.invalid, false);
  await f.wb.flush();
  assert.equal(JSON.parse(f.disk().text).title, "Demo");
});
test("field changes and undo do not mutate earlier React snapshots", async () => {
  const f = await fixture(),
    before = f.wb.getSnapshot();
  f.wb.set(["title"], "After");
  assert.equal(JSON.parse(before.doc!.text).title, "Demo");
  f.wb.undo();
  assert.equal(JSON.parse(f.wb.getSnapshot().doc!.text).title, "Demo");
  f.wb.undo(true);
  assert.equal(JSON.parse(f.wb.getSnapshot().doc!.text).title, "After");
});
