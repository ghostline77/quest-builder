import { useEffect, useRef, useState } from "react";
import { AppShell, Select, Input } from "@ghostline/ui-core";
import { Action } from "./fields";
import {
  Activity,
  Advanced,
  build,
  compareDisk,
  history,
  newFile,
  projectSettings,
  reload,
} from "./advanced";
import { DialogHost } from "./dialogs";
import { Story } from "./story";
import { Cast, Locations, Scene } from "./views";
import { human, kind, nameOf, object, text } from "./model";
import { useWorkbench, workbench as wb } from "./store";
let initialized = false;
export function QuestBuilder() {
  const state = useWorkbench(),
    {
      bootstrap,
      root,
      doc,
      section,
      advanced,
      jobs,
      conflict,
      saving,
      loading,
      notice,
    } = state;
  const [menu, setMenu] = useState(false),
    [search, setSearch] = useState("");
  const menuRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!initialized) {
      initialized = true;
      wb.run(() => wb.boot());
    }
    let polling = false;
    const timer = setInterval(() => {
      if (polling || !wb.getSnapshot().bootstrap) return;
      polling = true;
      wb.poll()
        .catch(wb.report)
        .finally(() => {
          polling = false;
        });
    }, 1800);
    const key = (e: KeyboardEvent): void => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
        e.preventDefault();
        wb.run(() => wb.save());
      }
      if (
        (e.ctrlKey || e.metaKey) &&
        e.key.toLowerCase() === "z" &&
        !(
          e.target instanceof HTMLInputElement ||
          e.target instanceof HTMLTextAreaElement ||
          (e.target instanceof HTMLElement && e.target.isContentEditable)
        )
      ) {
        e.preventDefault();
        wb.undo(e.shiftKey);
      }
      if (e.key === "Escape") setMenu(false);
    };
    const unload = (e: BeforeUnloadEvent): void => {
      if (wb.getSnapshot().doc?.dirty) {
        e.preventDefault();
        e.returnValue = "";
      }
    };
    const click = (e: MouseEvent): void => {
      if (e.target instanceof Node && !menuRef.current?.contains(e.target))
        setMenu(false);
    };
    document.addEventListener("keydown", key);
    document.addEventListener("click", click);
    window.addEventListener("beforeunload", unload);
    return () => {
      clearInterval(timer);
      document.removeEventListener("keydown", key);
      document.removeEventListener("click", click);
      window.removeEventListener("beforeunload", unload);
    };
  }, []);
  const running = jobs.filter((j) => j.status === "running").length,
    type = kind(doc?.path || "", doc?.value);
  const title = advanced
    ? doc
      ? nameOf(doc.value) === "Untitled"
        ? doc.path.split("/").at(-1)
        : nameOf(doc.value)
      : "Project files"
    : section === "story"
      ? text(object(wb.quest?.value).title, "Your quests")
      : doc &&
          ((section === "scenes" && ["scene", "dialogue"].includes(type)) ||
            (section === "locations" && type === "world") ||
            (section === "cast" && type === "character"))
        ? nameOf(doc.value)
        : human(section);
  const status = conflict
    ? "File changed on disk"
    : doc?.invalid
      ? "Invalid JSON · not saved"
      : saving
        ? "Saving…"
        : doc?.dirty
          ? "Unsaved changes"
          : running
            ? `${running} jobs running`
            : doc
              ? "Saved"
              : "Local authoring";
  const menus: [string, () => void | Promise<unknown>][] = [
    ["All fields", () => wb.advanced("fields")],
    ["Source JSON", () => wb.advanced("source")],
    ["File browser", () => wb.advanced("overview")],
    [
      "Quest graph",
      async () => {
        if (state.questPath && doc?.path !== state.questPath)
          await wb.open(state.questPath);
        wb.advanced("flow");
      },
    ],
    [
      "Journal & objectives",
      async () => {
        if (state.questPath && doc?.path !== state.questPath)
          await wb.open(state.questPath);
        wb.advanced("journal");
      },
    ],
    [
      "Contacts & messages",
      async () => {
        if (state.questPath && doc?.path !== state.questPath)
          await wb.open(state.questPath);
        wb.advanced("contacts");
      },
    ],
    [
      "Scene graph & commands",
      async () => {
        if (type !== "scene") {
          const scene = wb.documents("scene")[0];
          if (!scene) {
            await wb.navigate("scenes");
            return;
          }
          await wb.open(scene.path);
        }
        wb.advanced("graph");
      },
    ],
    ["References", () => wb.advanced("references")],
    ["Compiler tools", () => wb.update({ bottom: "tools" })],
    ["Activity & issues", () => wb.update({ bottom: "activity" })],
    ["Project settings", projectSettings],
    ["Save history", history],
    ["Undo", () => wb.undo()],
    ["Redo", () => wb.undo(true)],
    ["Reload from disk", reload],
    ["New authoring file", newFile],
  ];
  return (
    <AppShell
      className="h-dvh overflow-hidden"
      product="Quest builder"
      project={
        <div className="flex flex-wrap gap-4">
          <label className="flex items-center gap-2 text-sm">
            Project
            <Select
              aria-label="Project"
              disabled={loading}
              value={root}
              onChange={(e) => {
                const nextValue = e.target.value;
                return wb.run(async () => {
                  await wb.switchRoot(Number(nextValue));
                  if (wb.getSnapshot().questPath)
                    await wb.open(wb.getSnapshot().questPath);
                });
              }}
            >
              {bootstrap?.roots.map((r) => (
                <option key={r.id} value={r.id}>
                  {human(r.name.replaceAll("-", " "))}
                </option>
              ))}
            </Select>
          </label>
          <label className="flex items-center gap-2 text-sm">
            Quest
            <Select
              aria-label="Quest"
              disabled={loading}
              value={state.questPath}
              onChange={(e) => {
                const nextValue = e.target.value;
                if (nextValue)
                  wb.run(async () => {
                    await wb.open(nextValue);
                    wb.update({ advanced: false });
                  });
              }}
            >
              <option value="">
                {loading ? "Loading quests…" : "Choose a quest"}
              </option>
              {wb.documents("quest", false).map((r) => (
                <option key={r.path} value={r.path}>
                  {nameOf(r.value)}
                </option>
              ))}
            </Select>
          </label>
        </div>
      }
      actions={
        <>
          <span role="status" className="mr-2 text-xs text-muted">
            {bootstrap ? status : "Connecting…"}
          </span>
          <Action
            disabled={
              !doc?.value || !!doc.media || type === "character" || loading
            }
            onClick={() => wb.validate()}
          >
            Check{" "}
            {type === "scene"
              ? "scene"
              : type === "world"
                ? "location"
                : "quest"}
          </Action>
          <Action
            variant="primary"
            disabled={
              !doc ||
              !["quest", "scene", "world"].includes(type) ||
              !!doc.media ||
              !!running ||
              loading
            }
            onClick={build}
          >
            {running ? "Building…" : "Build"}
          </Action>
          <div ref={menuRef} className="relative">
            <Action
              aria-expanded={menu}
              aria-controls="advanced-menu"
              onClick={() => setMenu(!menu)}
            >
              Advanced ▾
            </Action>
            {menu && (
              <div
                id="advanced-menu"
                className="absolute right-0 z-30 mt-2 grid max-h-[70dvh] w-64 gap-1 overflow-auto rounded-lg border border-line bg-panel p-2 shadow-xl"
              >
                {menus.map(([label, fn]) => (
                  <Action
                    key={label}
                    variant="ghost"
                    className="justify-start"
                    onClick={() => {
                      setMenu(false);
                      return fn();
                    }}
                  >
                    {label}
                  </Action>
                ))}
              </div>
            )}
          </div>
        </>
      }
    >
      <nav
        aria-label="Quest sections"
        className="flex shrink-0 flex-wrap gap-2 border-b border-line px-4 py-2 sm:px-6"
      >
        {(["story", "scenes", "locations", "cast"] as const).map((s) => (
          <Action
            key={s}
            variant={section === s && !advanced ? "primary" : "ghost"}
            aria-current={section === s && !advanced ? "page" : undefined}
            onClick={() => wb.navigate(s)}
          >
            {human(s)}
          </Action>
        ))}
        {advanced && (
          <Action className="ml-auto" onClick={() => wb.navigate(section)}>
            Return to {section}
          </Action>
        )}
      </nav>
      <div className="flex min-h-0 flex-1 overflow-hidden">
        {advanced && (
          <aside
            aria-label="Project files"
            className="w-64 shrink-0 overflow-auto border-r border-line bg-panel/40 p-4 max-md:w-44"
          >
            <label className="grid gap-2 text-xs text-muted">
              Project files
              <Input
                type="search"
                aria-label="Find files"
                placeholder="Find files…"
                value={search}
                onChange={(e) => setSearch(e.target.value)}
              />
            </label>
            <div className="my-3 flex flex-wrap gap-2">
              <Action size="sm" onClick={newFile}>
                New file
              </Action>
              <Action size="sm" onClick={() => wb.refresh()}>
                Refresh
              </Action>
            </div>
            <nav className="space-y-1">
              {state.files
                .find((f) => f.id === root)
                ?.files.filter((f) =>
                  f.path.toLowerCase().includes(search.toLowerCase()),
                )
                .map((f) => (
                  <button
                    type="button"
                    key={f.path}
                    title={f.path}
                    onClick={() => wb.run(() => wb.open(f.path))}
                    className={`block w-full break-all rounded p-2 text-left text-xs hover:bg-surface ${doc?.path === f.path ? "bg-surface text-accent" : "text-muted"}`}
                  >
                    {f.path}
                  </button>
                ))}
            </nav>
          </aside>
        )}
        <main className="min-w-0 flex-1 overflow-auto" aria-busy={loading}>
          <div className="mx-auto max-w-6xl space-y-6 p-4 sm:p-8">
            <header className="flex flex-wrap items-start justify-between gap-4">
              <div>
                <p className="mb-2 text-xs uppercase tracking-widest text-muted">
                  {advanced ? "Advanced editor" : "Quest authoring"}
                </p>
                <h1 className="text-3xl font-semibold tracking-tight sm:text-4xl">
                  {title}
                </h1>
                <p className="mt-3 break-all text-sm text-muted">
                  {advanced ? doc?.path : text(object(doc?.value).description)}
                </p>
              </div>
              {doc && (
                <div className="flex flex-wrap gap-2">
                  <Action
                    size="sm"
                    disabled={!state.history.length}
                    onClick={() => wb.undo()}
                  >
                    Undo
                  </Action>
                  <Action
                    size="sm"
                    disabled={!state.future.length}
                    onClick={() => wb.undo(true)}
                  >
                    Redo
                  </Action>
                  <Action
                    size="sm"
                    disabled={!doc.dirty || doc.invalid || saving || conflict}
                    onClick={() => wb.save()}
                  >
                    Save
                  </Action>
                  {advanced && (
                    <Action size="sm" onClick={reload}>
                      Reload from disk
                    </Action>
                  )}
                </div>
              )}
            </header>
            {conflict && (
              <div className="flex flex-wrap items-center gap-3 rounded border border-amber-400/50 bg-amber-400/10 p-4">
                <p className="flex-1">
                  This file changed outside the editor. Compare versions before
                  saving.
                </p>
                <Action onClick={compareDisk}>Compare versions</Action>
              </div>
            )}
            {!!state.indexErrors.length && (
              <details className="rounded border border-red-400/30 p-3 text-red-200">
                <summary>
                  {state.indexErrors.length} authoring files could not be
                  indexed
                </summary>
                {state.indexErrors.map((e) => (
                  <p className="mt-2 text-xs" key={e}>
                    {e}
                  </p>
                ))}
              </details>
            )}
            {loading ? (
              <p className="py-16 text-center text-muted">Loading project…</p>
            ) : advanced || doc?.media ? (
              <Advanced key={`${root}:${doc?.path}`} />
            ) : section === "story" ? (
              <Story key={`${root}:${state.questPath}`} />
            ) : section === "scenes" ? (
              <Scene key={`${root}:${doc?.path}`} />
            ) : section === "locations" ? (
              <Locations key={`${root}:${doc?.path}`} />
            ) : (
              <Cast key={`${root}:${doc?.path}`} />
            )}
          </div>
        </main>
      </div>
      <Activity />
      {notice && (
        <div
          role={notice.bad ? "alert" : "status"}
          className={`fixed bottom-5 right-5 z-40 max-w-lg rounded-lg border p-4 shadow-xl ${notice.bad ? "border-red-400 bg-red-950 text-red-100" : "border-line bg-panel text-foreground"}`}
        >
          {notice.message}
          {!bootstrap && (
            <Action className="ml-3" onClick={() => wb.boot()}>
              Retry
            </Action>
          )}
        </div>
      )}
      <DialogHost />
    </AppShell>
  );
}
