import { test, expect } from "@playwright/test";
import type { Page } from "@playwright/test";
const questPath = "quests/demo/quest.json";
async function fixture(page: Page) {
  const quest = {
    schema_version: 1,
    id: "demo",
    title: "Demo quest",
    description: "A playable route",
    composition: {
      objectives: {
        meet: { text: "Meet Iris", description: "Find Iris at the bar" },
      },
    },
    stages: [
      {
        id: "meeting",
        title: "Meeting",
        type: "meet_contact",
        contact: "iris",
        community: "#bar",
        scene: "mod\\demo\\scenes\\intro.scene",
        scene_origin: "#bar_scene",
        setup_trigger: "#bar_area",
        objective: "@objectives.meet.path",
        on: { success: "wait" },
      },
      { id: "wait", title: "Wait for the call", type: "time_gate", seconds: 3 },
    ],
    resources: {
      scenes: [
        {
          spec: "scenes/intro.scene-spec.json",
          dialogue: "script/intro.dialogue.json",
        },
      ],
      worlds: ["world/bar.world.json"],
    },
  };
  const files = new Map<string, { text: string; hash: string }>(
    Object.entries({
      [questPath]: quest,
      "quests/demo/scenes/intro.scene-spec.json": {
        name: "intro",
        title: "Introduction",
        manifest: "../script/intro.dialogue.json",
        actors: [{ key: "v", kind: "player", name: "V" }],
        start_node: { key: "start", on_start: [{ node: "hello" }] },
        end_node: { key: "end" },
        sections: [
          { key: "hello", lines: ["hello"], on_end: [{ node: "end" }] },
        ],
        choices: [],
      },
      "quests/demo/script/intro.dialogue.json": {
        sequence_id: "intro",
        spoken_lines: [
          {
            key: "hello",
            speaker: "Iris",
            text: "Welcome to Night City",
            duration_ms: 1500,
          },
        ],
        choice_lines: [],
      },
      "quests/demo/world/bar.world.json": {
        name: "bar",
        title: "The bar",
        prefab_root: "$/mod/demo/#bar",
        origin: { x: 10, y: 20, z: 3, yaw: 90 },
        markers: [
          { ref: "#bar_scene", position: { from: "origin", forward: 4 } },
        ],
        triggers: [
          {
            ref: "#bar_area",
            position: { from: "origin" },
            outline: { type: "circle", radius: 3 },
          },
        ],
        entities: [],
      },
      "characters/iris.character.json": {
        id: "iris",
        name: "Iris",
        appearances: [{ id: "default", name: "Default" }],
        active_appearance: "default",
        loadout: [],
        ai: {},
      },
    }).map(([path, value]) => [
      path,
      { text: JSON.stringify(value, null, 2), hash: "initial-" + path },
    ]),
  );
  const writes: { path: string; text: string; hash: string | null }[] = [],
    jobs: {
      id: string;
      command: string;
      args: string[];
      status: string;
      stdout: string;
      stderr: string;
    }[] = [];
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.route("**/api/**", async (route) => {
    const endpoint = new URL(route.request().url()).pathname.slice(5),
      body = route.request().postDataJSON() as {
        path?: string;
        text?: string;
        hash?: string | null;
        command?: string;
        args?: string[];
        version?: string;
      } | null;
    const respond = (data: unknown, status = 200) =>
      route.fulfill({
        status,
        contentType: "application/json",
        body: JSON.stringify(data),
      });
    if (endpoint === "bootstrap")
      return respond({
        token: "test-token",
        roots: [{ id: 0, name: "Demo project", path: "K:/demo" }],
        schema: {},
        commands: [
          {
            name: "compile",
            about: "Compile a quest",
            args: [
              {
                id: "spec",
                long: null,
                positional: true,
                required: true,
                flag: false,
                multiple: false,
                defaults: [],
              },
              {
                id: "out",
                long: "out",
                positional: false,
                required: true,
                flag: false,
                multiple: false,
                defaults: [],
              },
            ],
          },
          { name: "validate", about: "Validate a quest", args: [] },
        ],
      });
    expect(route.request().headers()["x-quest-token"]).toBe("test-token");
    if (endpoint === "catalog")
      return respond({
        definitions: {},
        blocks: [
          {
            type: "time_gate",
            fields: {
              seconds: {
                required: true,
                schema: { type: "number", minimum: 0 },
              },
            },
          },
          {
            type: "meet_contact",
            fields: {
              contact: { required: true, schema: { type: "string" } },
              community: { required: true, schema: { type: "string" } },
              scene: { required: true, schema: { type: "string" } },
            },
          },
        ],
      });
    if (endpoint === "files")
      return respond([
        {
          id: 0,
          files: [...files].map(([path, f]) => ({ path, size: f.text.length })),
        },
      ]);
    if (endpoint === "read")
      return respond(
        files.get(body?.path || "") || { error: "Missing file" },
        files.has(body?.path || "") ? 200 : 400,
      );
    if (endpoint === "save") {
      const existing = files.get(body!.path!);
      if (existing && body!.hash !== existing.hash)
        return respond(
          { error: "file changed on disk; reload or compare before saving" },
          400,
        );
      writes.push({ path: body!.path!, text: body!.text!, hash: body!.hash! });
      const hash = "saved-" + writes.length;
      files.set(body!.path!, { text: body!.text!, hash });
      return respond({ hash });
    }
    if (endpoint === "jobs" && route.request().method() === "POST") {
      const id = "job-" + jobs.length;
      jobs.push({
        id,
        command: body!.command!,
        args: body!.args!,
        status: "running",
        stdout: "",
        stderr: "",
      });
      return respond({ id });
    }
    if (endpoint === "jobs") return respond(jobs);
    if (endpoint === "history")
      return respond(
        body?.version
          ? { text: JSON.stringify(quest) }
          : { versions: ["1794820000000000000"] },
      );
    return respond({});
  });
  await page.goto("/");
  await expect(
    page.getByRole("heading", { name: "Demo quest", exact: true }),
  ).toBeVisible();
  return { files, writes, jobs, errors };
}
async function advanced(page: Page, name: string) {
  await page.getByRole("button", { name: "Advanced ▾", exact: true }).click();
  await page
    .locator("#advanced-menu")
    .getByRole("button", { name, exact: true })
    .click();
}

test("React stage editing, linked objectives, undo/redo and autosave", async ({
  page,
}) => {
  const f = await fixture(page);
  await page.getByRole("button", { name: "Meet Iris", exact: true }).click();
  await page
    .getByLabel("Stage title", { exact: true })
    .fill("Meet Iris downstairs");
  await expect(
    page.getByRole("button", { name: "Meet Iris downstairs", exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Undo", exact: true }).click();
  await expect(page.getByLabel("Stage title", { exact: true })).toHaveValue(
    "Meet Iris",
  );
  await page.getByRole("button", { name: "Redo", exact: true }).click();
  await page.getByRole("button", { name: "Save", exact: true }).click();
  await expect.poll(() => f.writes.length).toBe(1);
  expect(JSON.parse(f.writes[0]!.text).composition.objectives.meet.text).toBe(
    "Meet Iris downstairs",
  );
  expect(f.errors).toEqual([]);
});
test("add and reorder a stage, preserve explicit outcome", async ({ page }) => {
  const f = await fixture(page);
  await page.getByRole("button", { name: "+ Add stage", exact: true }).click();
  await page.getByLabel("Stage type").selectOption("time_gate");
  await page.getByLabel("Stage name").fill("Wait again");
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "Add stage", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Wait again", exact: true }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "Move Wait again up", exact: true })
    .click();
  await page.getByRole("button", { name: "Save", exact: true }).click();
  await expect.poll(() => f.writes.length).toBeGreaterThan(0);
  const data = JSON.parse(f.writes.at(-1)!.text);
  expect(data.stages.map((s: { id: string }) => s.id)).toEqual([
    "meeting",
    "wait_again",
    "wait",
  ]);
  expect(data.stages[0].on.success).toBe("wait");
  expect(f.errors).toEqual([]);
});
test("source JSON rejects invalid input and preserves 64-bit IDs", async ({
  page,
}) => {
  const f = await fixture(page);
  await advanced(page, "Source JSON");
  const source = page.getByLabel("Document source");
  await source.fill("{");
  await expect(
    page.getByRole("button", { name: "Save", exact: true }),
  ).toBeDisabled();
  await source.fill(
    '{"id":"demo","title":"Demo quest","stages":[],"numeric_id":18446744073709551615}',
  );
  await page.getByRole("button", { name: "Save", exact: true }).click();
  await expect.poll(() => f.writes.length).toBe(1);
  expect(f.writes[0]!.text).toContain("18446744073709551615");
  expect(f.errors).toEqual([]);
});
test("disk conflict requires comparison and saves using reviewed hash", async ({
  page,
}) => {
  const f = await fixture(page);
  await page.getByRole("button", { name: "Meet Iris", exact: true }).click();
  await page.getByLabel("Stage title", { exact: true }).fill("Editor text");
  const disk = f.files.get(questPath)!;
  f.files.set(questPath, {
    ...disk,
    hash: "external-change",
    text: disk.text.replace("Demo quest", "External quest"),
  });
  await page.getByRole("button", { name: "Save", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Compare versions", exact: true }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "Compare versions", exact: true })
    .click();
  await expect(page.getByRole("dialog")).toBeVisible();
  await page.getByRole("button", { name: "Apply choice", exact: true }).click();
  await expect.poll(() => f.writes.length).toBe(1);
  expect(f.writes[0]!.hash).toBe("external-change");
  expect(f.writes[0]!.text).toContain("Editor text");
});
test("scene dialogue editing, locations map and cast use React views", async ({
  page,
}) => {
  const f = await fixture(page);
  await page.getByRole("button", { name: "Scenes", exact: true }).click();
  await page.getByRole("button", { name: /Introduction/ }).click();
  await expect(
    page.getByText("Welcome to Night City", { exact: true }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "Write dialogue", exact: true })
    .click();
  await page
    .getByLabel("Dialogue · line 1", { exact: true })
    .fill("New dialogue wording");
  await page.getByRole("button", { name: "Save", exact: true }).click();
  await expect.poll(() => f.writes.length).toBe(1);
  await page.getByRole("button", { name: "Locations", exact: true }).click();
  await page.getByRole("button", { name: /The bar/ }).click();
  await page.getByText("Placement map", { exact: true }).click();
  await expect(
    page.getByRole("img", { name: "Local quest placement editor" }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Cast", exact: true }).click();
  await page.getByRole("button", { name: /Iris/ }).click();
  await expect(page.getByRole("group").filter({ has: page.locator("summary", { hasText: /^Character$/ }) }).getByLabel("Name", { exact: true })).toHaveValue("Iris");
  expect(f.errors).toEqual([]);
});
test("build flushes edits and posts documents-only job; activity polls results", async ({
  page,
}) => {
  const f = await fixture(page);
  await page.getByRole("button", { name: "Meet Iris", exact: true }).click();
  await page.getByLabel("Stage title", { exact: true }).fill("Before build");
  await page.getByRole("button", { name: "Build", exact: true }).click();
  await page.getByLabel("Build output").selectOption("documents");
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "Continue", exact: true })
    .click();
  await expect.poll(() => f.jobs.length).toBe(1);
  expect(f.writes.length).toBeGreaterThan(0);
  expect(f.jobs[0]!.args).toContain("--documents-only");
  expect(f.jobs[0]!.args[0]).toBe(questPath);
  f.jobs[0]!.status = "succeeded";
  f.jobs[0]!.stdout = '{"valid":true}';
  await expect(page.getByText("succeeded", { exact: true })).toBeVisible();
  expect(f.errors).toEqual([]);
});
test("layout fits mobile and desktop, menu and graph are usable", async ({
  page,
}) => {
  const f = await fixture(page);
  await page.getByRole("button", { name: "Graph", exact: true }).click();
  await expect(page.getByRole("img", { name: "Quest graph" })).toBeVisible();
  await page.screenshot({
    path: "test-results/quest-desktop.png",
    fullPage: true,
  });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.getByRole("button", { name: "Outline", exact: true }).click();
  await page.screenshot({
    path: "test-results/quest-mobile.png",
    fullPage: true,
  });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  expect(f.errors).toEqual([]);
});
