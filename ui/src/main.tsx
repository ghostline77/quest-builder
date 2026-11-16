import { AppShell, Button, Input, Select, mountApp } from "@ghostline/ui-core";
import "./style.css";

function QuestBuilder() {
  return (
    <AppShell
      product="Quest builder"
      project={
        <>
          <div className="project-picker">
            <label htmlFor="workspace">Project</label>
            <Select id="workspace" aria-label="Project" />
          </div>
          <div className="quest-picker">
            <label htmlFor="quest-select">Quest</label>
            <Select id="quest-select" aria-label="Quest" />
          </div>
          <span id="workspace-label" hidden />
        </>
      }
      actions={
        <div className="header-actions">
          <span id="save-status" role="status">
            Connecting…
          </span>
          <Button id="validate">Check quest</Button>
          <Button id="build" variant="primary">
            Build
          </Button>
          <div className="advanced-menu">
            <Button
              id="advanced-menu-button"
              aria-expanded="false"
              aria-controls="advanced-menu"
            >
              Advanced
            </Button>
            <div id="advanced-menu" hidden />
          </div>
        </div>
      }
    >
      <nav id="project-nav" aria-label="Quest sections" />
      <div id="advanced-banner" hidden>
        <span>Advanced editor</span>
        <Button id="return-to-story">Return to story</Button>
      </div>
      <div className="shell">
        <aside id="sidebar">
          <div className="sidebar-top">
            <label className="eyebrow" htmlFor="search">
              Project files
            </label>
            <Input
              id="search"
              type="search"
              placeholder="Find quests, scenes, assets…"
              aria-label="Find files"
            />
            <div className="row">
              <Button id="new-file">New file</Button>
              <Button id="refresh">Refresh</Button>
            </div>
          </div>
          <nav id="files" aria-label="Project files" />
          <div className="sidebar-footer">Authoring sources</div>
        </aside>
        <main>
          <div className="document-bar">
            <div>
              <div className="eyebrow" id="document-type">
                Quest authoring
              </div>
              <h1 id="title">Your quests</h1>
              <div id="path" className="muted" />
            </div>
            <div className="row">
              <Button id="compare" hidden>
                Compare disk
              </Button>
              <Button id="reload" hidden>
                Reload from disk
              </Button>
            </div>
          </div>
          <nav id="views" className="tabs" aria-label="Editor views" />
          <div id="editor" />
        </main>
        <aside id="inspector">
          <div className="inspector-heading">
            <span className="eyebrow">Inspector</span>
            <Button id="clear-selection" aria-label="Show document properties">
              Document
            </Button>
          </div>
          <div id="properties" />
        </aside>
      </div>
      <section id="bottom" hidden>
        <div className="bottom-heading">
          <nav className="tabs">
            <Button data-bottom="activity" className="active">
              Activity <span id="job-count">0</span>
            </Button>
            <Button data-bottom="diagnostics">
              Issues <span id="diagnostic-count">0</span>
            </Button>
            <Button data-bottom="tools">Compiler tools</Button>
          </nav>
          <div className="row">
            <Button id="toggle-bottom" aria-label="Expand activity panel">
              Expand
            </Button>
            <Button id="close-bottom" aria-label="Close activity panel">
              Close
            </Button>
          </div>
        </div>
        <div id="bottom-content" />
      </section>
      <div id="toast" role="status" hidden />
      <dialog id="dialog">
        <form method="dialog">
          <div id="dialog-content" />
          <div className="dialog-actions">
            <Button type="submit" value="cancel">
              Cancel
            </Button>
            <Button
              id="dialog-confirm"
              type="submit"
              value="ok"
              variant="primary"
            >
              Continue
            </Button>
          </div>
        </form>
      </dialog>
      <div id="editor-actions" hidden>
        <Button id="undo" aria-label="Undo">
          Undo
        </Button>
        <Button id="redo" aria-label="Redo">
          Redo
        </Button>
        <Button id="save">Save</Button>
      </div>
    </AppShell>
  );
}
mountApp(<QuestBuilder />, () => import("../app.js"));
