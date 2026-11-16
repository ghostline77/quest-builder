import { createAuthoringUI } from './story.js';

const $ = s => document.querySelector(s);
const el = (tag, cls, text) => { const n = document.createElement(tag); if (cls) n.className = cls; if (text !== undefined) n.textContent = text; return n; };
const button = (text, fn, cls = '') => { const n = el('button', cls, text); n.type = 'button'; n.onclick = () => Promise.resolve().then(fn).catch(error); return n; };
const state = { root: 0, files: [], doc: null, view: 'overview', selection: [], history: [], future: [], jobs: [], diagnostics: [], bottom: 'activity', command: 'compile', settings: {}, docs: new Map(), saveTimer: null, saving: false, conflict: false, scale: 10, pan: { x: 0, y: 0 } };
let bootstrap, catalog, token, authoring;
let fieldSequence = 0;
const pretty = v => JSON.stringify(v, null, 2);
function parse(text) {
  return JSON.parse(text, (key, value, context) => {
    if (typeof value === 'number' && Number.isInteger(value) && !Number.isSafeInteger(value)) {
      if (!JSON.rawJSON || !context?.source) throw Error('This browser cannot safely edit 64-bit numeric IDs. Use a current browser.');
      return JSON.rawJSON(context.source);
    }
    return value;
  });
}
const clone = value => parse(JSON.stringify(value));
const isRaw = value => value && typeof value === 'object' && Object.hasOwn(value, 'rawJSON');
const at = (path, value = state.doc?.value) => path.reduce((v, k) => v?.[k], value);
const pointer = path => '/' + path.map(k => String(k).replaceAll('~', '~0').replaceAll('/', '~1')).join('/');
const unpointer = p => p.replace(/^\//, '').split('/').filter(Boolean).map(s => s.replaceAll('~1', '/').replaceAll('~0', '~'));
const human = name => String(name).replaceAll('_', ' ').replace(/\b\w/g, s => s.toUpperCase());
const isMedia = path => /\.(png|jpe?g|webp|wav|ogg|mp3|mp4|webm)$/i.test(path);
const sceneCommands = ['appearance', 'character_gender', 'equip_actor', 'equip_player', 'journal', 'mappin', 'pause_condition', 'player_look_at', 'player_status_effect', 'puppet_ai', 'realtime_delay', 'render_fade', 'scene_tier', 'show_player', 'spawn_actor', 'stop_workspot', 'teleport_actor', 'teleport_player', 'unequip_actor', 'unequip_player', 'use_workspot', 'wait_actor_spawned'];
const nameOf = value => value?.title || value?.name || value?.key || value?.id || value?.ref || value?.type || 'Untitled';
function error(e) { console.error(e); toast(e.message || String(e), true); }
function toast(message, bad = false) { const n = $('#toast'); n.textContent = message; n.classList.toggle('error', bad); n.hidden = false; clearTimeout(n.timer); n.timer = setTimeout(() => n.hidden = true, bad ? 11000 : 4000); }
async function api(path, data) {
  const r = await fetch('/api/' + path, { method: data === undefined ? 'GET' : 'POST', headers: { 'content-type': 'application/json', 'x-quest-token': token }, body: data === undefined ? undefined : JSON.stringify(data) });
  const value = await r.json(); if (!r.ok) throw Error(value.error || `${r.status} ${r.statusText}`); return value;
}
async function ask(title, build, confirm = 'Continue') {
  const dialog = $('#dialog'), body = $('#dialog-content'); body.replaceChildren(el('h2', '', title));
  const result = build(body); $('#dialog-confirm').textContent = confirm; dialog.returnValue = ''; dialog.showModal();
  return new Promise(resolve => dialog.addEventListener('close', () => resolve(dialog.returnValue === 'ok' ? result() : null), { once: true }));
}
function inputField(parent, name, value = '', type = 'text') {
  const wrap = el('div', 'field'), label = el('label', '', name), input = el('input'); input.type = type; input.value = value; label.append(input); wrap.append(label); parent.append(wrap); return input;
}
function status() {
  const d = state.doc;
  $('#save-status').textContent = state.conflict ? 'Disk conflict · reload to resolve' : state.saving ? 'Saving…' : d?.dirty ? 'Unsaved changes' : d ? 'All changes saved' : 'Local authoring';
  $('#save').disabled = !d?.dirty || state.saving || state.conflict;
  $('#compare').hidden = !state.conflict;
  $('#undo').disabled = !state.history.length; $('#redo').disabled = !state.future.length;
  $('#validate').disabled = !d?.value; $('#build').disabled = !d || Boolean(d.media);
  authoring?.syncStatus();
}
function snapshot() { return state.doc?.value !== undefined ? pretty(state.doc.value) : state.doc?.text; }
function checkpoint() { if (!state.doc) return; state.history.push(snapshot()); if (state.history.length > 100) state.history.shift(); state.future = []; }
function changed(render = false) {
  state.doc.dirty = true; state.doc.text = state.doc.value !== undefined ? pretty(state.doc.value) + '\n' : state.doc.text;
  if (state.doc.value) $('#title').textContent = authoring && !state.advanced ? authoring.documentTitle() : nameOf(state.doc.value);
  clearTimeout(state.saveTimer); state.saveTimer = setTimeout(() => save().catch(error), 1300); status(); if (render) renderDocument();
}
function set(path, value, render = false) { checkpoint(); if (!path.length) state.doc.value = value; else at(path.slice(0, -1))[path.at(-1)] = value; changed(render); }
function remove(path) { checkpoint(); const parent = at(path.slice(0, -1)); if (Array.isArray(parent)) parent.splice(Number(path.at(-1)), 1); else delete parent[path.at(-1)]; changed(true); }
async function save() {
  clearTimeout(state.saveTimer); const d = state.doc;
  if (!d?.dirty || state.saving || state.conflict || d.invalid) return;
  state.saving = true; status(); const text = d.text;
  try { const result = await api('save', { root: d.root, path: d.path, text, hash: d.hash ?? null }); d.hash = result.hash; d.dirty = d.text !== text; state.docs.set(`${d.root}:${d.path}`, d); }
  catch (e) { if (e.message.includes('changed on disk') || e.message.includes('removed on disk')) state.conflict = true; throw e; }
  finally { state.saving = false; status(); if (d.dirty && !state.conflict) state.saveTimer = setTimeout(() => save().catch(error), 1800); }
}
async function flush() { if (state.doc?.invalid) throw Error('Fix invalid JSON before continuing.'); if (state.conflict) throw Error('Resolve the disk conflict before continuing.'); while (state.saving) await new Promise(r => setTimeout(r, 40)); await save(); }
function undo(redo = false) {
  const source = redo ? state.future : state.history, destination = redo ? state.history : state.future;
  if (!source.length) return; destination.push(snapshot()); const text = source.pop();
  if (state.doc.value !== undefined) state.doc.value = parse(text); else state.doc.text = text;
  changed(true);
}
function kind(path, value) {
  if (value?.stages || (value?.schema_version && value?.id && value?.title)) return 'quest';
  if (path.endsWith('.world.json') || value?.quest_sector_path || value?.prefab_root) return 'world';
  if (path.includes('scene-spec') || value?.sections && value?.actors) return 'scene';
  if (value?.spoken_lines || value?.choice_lines) return 'dialogue';
  if (path.includes('package')) return 'package';
  if (path.startsWith('characters/')) return 'character';
  if (path.includes('localization')) return 'localization';
  if (path.includes('scenario') || path.includes('evidence')) return 'playtest';
  return 'document';
}
async function openFile(path, root = state.root, force = false) {
  await flush();
  const key = `${root}:${path}`;
  if (state.doc) state.docs.set(`${state.doc.root}:${state.doc.path}`, state.doc);
  let d = !force && state.docs.get(key);
  if (!d && isMedia(path)) { const r = await fetch('/api/media', { method: 'POST', headers: { 'content-type': 'application/json', 'x-quest-token': token }, body: JSON.stringify({ root, path }) }); if (!r.ok) throw Error((await r.json()).error); d = { root, path, media: URL.createObjectURL(await r.blob()), dirty: false }; }
  if (!d) { const file = await api('read', { root, path }); let value; if (path.endsWith('.json')) { try { value = parse(file.text); } catch (e) { toast(`Opened as source: ${e.message}`, true); } } d = { ...file, root, path, value, dirty: false }; }
  state.root = root; state.doc = d; state.history = []; state.future = []; state.conflict = false; state.selection = []; state.scale = 0; state.pan = { x: 0, y: 0 };
  state.view = ({ quest: 'flow', world: 'placement', scene: 'graph', dialogue: 'dialogue' })[kind(path, d.value)] || 'overview';
  authoring?.onOpen(d);
  $('#workspace').value = root; localStorage.setItem('quest-workbench-last', JSON.stringify({ root: bootstrap.roots[root].path, path })); renderFiles(); renderDocument(); $('main').scrollTop = 0; $('#inspector').scrollTop = 0;
}
async function reload() {
  if (!state.doc) return;
  if (state.doc.dirty) { const okay = await ask('Reload from disk?', p => { p.append(el('p', 'muted', 'Unsaved edits in this document will be discarded. Saved versions remain in work/editor-history.')); return () => true; }, 'Reload'); if (!okay) return; }
  const { root, path } = state.doc; clearTimeout(state.saveTimer); state.doc.dirty = false; state.conflict = false; await openFile(path, root, true);
}
async function compareDisk() {
  const d = state.doc, disk = await api('read', { root: d.root, path: d.path });
  const result = await ask('Resolve external changes', p => { p.append(el('h3', '', 'Editor version')); const mine = el('textarea', 'source'); mine.value = d.text; mine.readOnly = true; mine.style.minHeight = '130px'; p.append(mine, el('h3', '', 'Version on disk')); const other = el('textarea', 'source'); other.value = disk.text; other.readOnly = true; other.style.minHeight = '130px'; p.append(other); const mode = el('select'); mode.append(new Option('Keep editor version and save over the reviewed disk version', 'mine'), new Option('Use disk version', 'disk')); p.append(mode); return () => mode.value; }, 'Apply choice');
  if (result === 'mine') { d.hash = disk.hash; state.conflict = false; await save(); }
  else if (result === 'disk') { d.dirty = false; state.conflict = false; await openFile(d.path, d.root, true); }
}
async function refresh() { state.files = await api('files'); renderFiles(); }
function group(path) {
  if (isMedia(path)) return 'Previews & source media';
  if (/quest\.json$|\.quest\.json$|series\.json$|project\.json$/.test(path)) return 'Quests & projects';
  if (/scenes?\//.test(path) || path.includes('scene-spec')) return 'Scenes';
  if (/world\//.test(path) || path.includes('.world.')) return 'Locations & placement';
  if (/script\//.test(path) || path.includes('dialogue')) return 'Dialogue & text';
  if (/characters?\//.test(path)) return 'Characters';
  if (/voice\//.test(path) || /voices\//.test(path) || /localization/.test(path)) return 'Localization & media';
  if (/package|release\//.test(path)) return 'Build & packaging';
  if (/scenario|evidence|playtest/.test(path)) return 'Playtesting';
  return 'Documents & assets';
}
function renderFiles() {
  const nav = $('#files'); nav.replaceChildren(); const query = $('#search').value.toLowerCase();
  const grouped = new Map(); for (const f of state.files.find(r => r.id === state.root)?.files || []) {
    if (!f.path.toLowerCase().includes(query)) continue; const g = group(f.path); if (!grouped.has(g)) grouped.set(g, []); grouped.get(g).push(f);
  }
  for (const [name, files] of grouped) { const detail = el('details'); detail.open = Boolean(query) || name === 'Quests & projects' || files.some(f => f.path === state.doc?.path); detail.append(el('summary', '', `${name} · ${files.length}`));
    for (const f of files) { const b = button(f.path, () => openFile(f.path), state.doc?.root === state.root && state.doc?.path === f.path ? 'active' : ''); b.title = f.path; detail.append(b); } nav.append(detail);
  }
  $('#workspace-label').textContent = bootstrap.roots[state.root].name;
}
function renderDocument() {
  authoring?.syncHeader();
  if (authoring && !state.advanced) { authoring.render(); status(); return; }
  const d = state.doc; $('#reload').hidden = !d; $('#views').replaceChildren(); $('#editor').replaceChildren();
  if (!d) { renderWelcome(); renderInspector(); status(); return; }
  const type = kind(d.path, d.value); $('#document-type').textContent = ({ world: 'LOCATIONS & QUEST PLACEMENT', quest: 'QUEST AUTHORING', scene: 'SCENE AUTHORING', dialogue: 'DIALOGUE & LOCALIZATION' })[type] || human(type).toUpperCase();
  $('#title').textContent = nameOf(d.value) === 'Untitled' ? d.path.split('/').at(-1) : nameOf(d.value); $('#path').textContent = d.path;
  if (d.media) { const media = el(/\.(png|jpe?g|webp)$/i.test(d.path) ? 'img' : /\.(mp4|webm)$/i.test(d.path) ? 'video' : 'audio'); media.src = d.media; media.alt = d.path; media.controls = true; media.style.maxWidth = '100%'; media.style.maxHeight = '65vh'; $('#editor').append(media); renderInspector(); status(); return; }
  let views = ['overview', 'fields', 'references', 'source'];
  if (type === 'quest') views = ['overview', 'flow', 'journal', 'contacts', 'fields', 'references', 'source'];
  if (type === 'scene') views = ['overview', 'graph', 'timeline', 'actors', 'fields', 'references', 'source'];
  if (type === 'world') views = ['placement', 'overview', 'fields', 'references', 'source'];
  if (type === 'dialogue') views = ['dialogue', 'overview', 'fields', 'references', 'source'];
  if (d.value === undefined) views = ['overview', 'source'];
  if (!views.includes(state.view)) state.view = views[0];
  for (const v of views) $('#views').append(button(human(v), () => { state.view = v; renderDocument(); }, state.view === v ? 'active' : ''));
  const editor = $('#editor');
  if (state.view === 'source') renderSource(editor);
  else if (state.view === 'fields') renderFields(editor, [], d.value, schemaFor([]), 0, true);
  else if (state.view === 'flow') renderFlow(editor);
  else if (state.view === 'graph') renderSceneGraph(editor);
  else if (state.view === 'timeline') renderTimeline(editor).catch(error);
  else if (state.view === 'placement') renderWorld(editor);
  else if (state.view === 'dialogue') renderDialogue(editor);
  else if (state.view === 'actors') renderCollection(editor, ['actors'], 'Actor');
  else if (state.view === 'journal') renderSubtree(editor, ['composition', 'objectives'], 'Objectives', {});
  else if (state.view === 'contacts') renderSubtree(editor, ['composition', 'contacts'], 'Contacts', {});
  else if (state.view === 'references') renderReferences(editor);
  else renderOverview(editor);
  renderInspector(); status();
}
function renderWelcome() {
  $('#document-type').textContent = 'AUTHORING WORKBENCH'; $('#title').textContent = 'Your next story starts here.'; $('#path').textContent = 'Open a quest, shape its flow, and build directly from source.';
  const parent = $('#editor'), welcome = el('div', 'welcome');
  const stats = el('div', 'stats'); for (const [count, label] of [[catalog.blocks.length, 'Building blocks'], [bootstrap.roots.length, 'Workspaces'], [state.files.reduce((a, r) => a + r.files.filter(f => /quest\.json$/.test(f.path)).length, 0), 'Quest manifests']]) { const n = el('div'); n.append(el('strong', '', count), el('span', '', label)); stats.append(n); } welcome.append(stats);
  const cards = el('div', 'welcome-grid');
  for (const [title, desc, fn] of [['Open a quest', 'Build encounters, branching stories and character moments.', () => { $('#search').value = 'quest.json'; renderFiles(); }], ['Create a document', 'Quest, scene, dialogue or a local placement group.', newFile], ['Building block library', 'Explore every native compiler block and its editable fields.', () => { state.bottom = 'tools'; state.command = 'catalog'; expandBottom(); renderBottom(); }], ['Build & inspect', 'Generate resources, package a release, and review evidence.', () => { state.bottom = 'tools'; expandBottom(); renderBottom(); }]]) { const b = button('', fn); b.append(el('span', 'eyebrow', 'WORKSPACE'), el('b', '', title), el('small', '', desc)); cards.append(b); }
  welcome.append(cards); parent.append(welcome);
}
function schemaFor(path) {
  if (path[0] === 'quest_nodes' && path.at(-1) === 'kind') return { enum: sceneCommands };
  if (kind(state.doc?.path || '', state.doc?.value) !== 'quest') return {};
  let schema = bootstrap.schema;
  for (let i = 0; i < path.length; i++) {
    const key = path[i]; schema = resolveSchema(schema);
    if (path[0] === 'stages' && i === 1) { const stage = at(path.slice(0, 2)); const block = catalog.blocks.find(b => b.type === stage?.type); schema = { type: 'object', properties: Object.fromEntries(Object.entries(block?.fields || {}).map(([k, f]) => [k, f.schema])), required: Object.entries(block?.fields || {}).filter(([, f]) => f.required).map(([k]) => k) }; }
    else schema = Array.isArray(at(path.slice(0, i))) ? schema.items || {} : schema.properties?.[key] || (typeof schema.additionalProperties === 'object' ? schema.additionalProperties : {});
  }
  return resolveSchema(schema);
}
function resolveSchema(schema = {}) {
  if (schema.$ref?.startsWith('#/')) schema = at(unpointer(schema.$ref.slice(1)), bootstrap.schema) || {};
  if (schema.allOf) { let props = {}, req = []; for (const s of schema.allOf) { const r = resolveSchema(s); Object.assign(props, r.properties); req.push(...r.required || []); } schema = { ...schema, properties: props, required: [...new Set(req)] }; }
  return schema;
}
function defaultValue(schema = {}, key = '') {
  schema = resolveSchema(schema); if (schema.default !== undefined) return clone(schema.default); if (schema.const !== undefined) return schema.const; if (schema.enum) return schema.enum[0];
  if (schema.oneOf || schema.anyOf) return defaultValue((schema.oneOf || schema.anyOf)[0], key);
  if (schema.type === 'object' || schema.properties) return Object.fromEntries((schema.required || []).map(k => [k, defaultValue(schema.properties?.[k], k)]));
  if (schema.type === 'array' || schema.items) return []; if (schema.type === 'boolean') return false; if (['number', 'integer'].includes(schema.type)) return schema.minimum || 0;
  if (['on', 'outcomes', 'composition', 'resources', 'position', 'origin', 'outline', 'node_data'].includes(key)) return {};
  if (['stages', 'actors', 'sections', 'choices', 'spoken_lines', 'choice_lines', 'markers', 'triggers', 'entities', 'devices', 'communities', 'quest_nodes', 'start_nodes', 'end_nodes', 'entry_points', 'exit_points', 'phase_prefabs', 'required_assets', 'lines', 'options', 'on_start', 'on_out', 'on_end', 'grants', 'performance'].includes(key)) return [];
  return '';
}
function arrayEntry(path, schema) {
  if (schema.items && Object.keys(schema.items).length) return defaultValue(schema.items);
  const name = path.at(-1), n = (at(path)?.length || 0) + 1;
  return ({ actors: { key: `actor_${n}`, kind: 'community', community_ref: '', entry: '' }, sections: { key: `section_${n}`, lines: [], on_end: [] }, choices: { key: `choice_${n}`, options: [] }, options: { choice_key: '', caption: '', target: '' }, spoken_lines: { key: `line_${n}`, speaker: '', text: '', duration_ms: 0 }, choice_lines: { key: `choice_${n}`, text: '' }, entry_points: { name: `entry_${n}`, node: '' }, exit_points: { name: `exit_${n}`, node: '' }, quest_nodes: { key: `command_${n}`, kind: 'realtime_delay', duration_ms: 1000, on_out: [] }, start_nodes: { key: `start_${n}`, on_start: [] }, end_nodes: { key: `end_${n}` }, markers: { ref: `#marker_${n}`, position: { from: 'origin' } }, triggers: { ref: `#trigger_${n}`, position: { from: 'origin' }, outline: { type: 'circle', radius: 3, height: 3 }, notifiers: ['quest'] }, on_start: { node: '' }, on_out: { node: '' }, on_end: { node: '' }, entities: { ref: `#entity_${n}`, template: '', position: { from: 'origin' } }, devices: { ref: `#device_${n}`, kind: 'access_point', template: '', position: { from: 'origin' } } })[name] ?? (typeof at(path)?.[0] === 'object' ? {} : '');
}
function select(path) { state.selection = path; if (authoring && !state.advanced) { authoring.onSelect(path); renderDocument(); return; } document.body.classList.add('inspecting'); renderInspector(); if (['flow', 'graph', 'placement'].includes(state.view)) renderDocument(); }
function renderInspector() {
  const parent = $('#properties'); parent.replaceChildren(); if (!state.doc?.value) { parent.append(el('p', 'muted', 'Select a quest, scene or placement to inspect its properties.')); return; }
  let path = state.selection; if (at(path) === undefined) path = state.selection = [];
  const value = at(path); parent.append(el('h3', '', path.length ? value !== null && typeof value === 'object' ? nameOf(value) : human(path.at(-1)) : 'Document properties'), el('p', 'muted', pointer(path) || '/'));
  if (!path.length) parent.append(button('Saved versions…', showHistory, 'link-button'));
  if (path.length) { const actions = el('div', 'toolbar'); actions.append(button('References', () => { state.view = 'references'; renderDocument(); }), button('Delete', async () => { const refs = referencesTo(nameOf(value)); const ok = await ask('Delete selected entry?', p => { p.append(el('p', 'muted', `${refs.length} matching reference(s) in this document. Review references before deleting an entry used elsewhere.`)); return () => true; }, 'Delete'); if (ok) { remove(path); state.selection = []; renderDocument(); } }, 'danger')); parent.append(actions); }
  if (path[0] === 'stages' && path.length === 2) renderRouting(parent, path, value);
  renderFields(parent, path, value, schemaFor(path), 0, true);
}
function renderFields(parent, path, value, schema = {}, depth = 0, expanded = false) {
  schema = resolveSchema(schema);
  if (value !== null && typeof value === 'object' && !isRaw(value)) {
    const array = Array.isArray(value); let body = parent;
    if (depth > 0) { const group = el('details', 'field-group'); group.open = expanded; group.append(el('summary', '', `${human(path.at(-1))} · ${array ? value.length + ' entries' : Object.keys(value).length + ' fields'}`)); body = el('div', 'nested'); group.append(body); parent.append(group); }
    for (const [key, child] of Object.entries(value)) {
      const childPath = [...path, key], childSchema = resolveSchema(array ? schema.items : schema.properties?.[key]);
      const wrap = el('div', array ? 'card' : '');
      if (array) { const head = el('div', 'item-heading'); head.append(button(`${Number(key) + 1}. ${nameOf(child)}`, () => select(childPath), 'link-button')); const actions = el('div', 'row'); actions.append(button('↑', () => moveItem(path, Number(key), -1)), button('↓', () => moveItem(path, Number(key), 1)), button('×', () => remove(childPath))); head.append(actions); wrap.append(head); }
      if (child !== null && typeof child === 'object' && !isRaw(child)) renderFields(wrap, childPath, child, childSchema, depth + 1);
      else renderScalar(wrap, childPath, child, childSchema, schema.required?.includes(key));
      body.append(wrap);
    }
    if (!array && Object.hasOwn(value, 'node') && !Object.hasOwn(value, 'input')) {
      const field = el('div', 'field'), label = el('label', '', 'Input socket'), input = el('input'); input.placeholder = 'In (default)'; input.setAttribute('aria-label', 'Input socket');
      input.oninput = () => { checkpoint(); const socket = input.value.trim(); if (!socket || socket === 'In') delete value.input; else value.input = socket; changed(); }; label.append(input); field.append(label); body.append(field);
    }
    const add = el('div', 'row'); add.append(button(array ? '+ Add entry' : '+ Add field', () => addField(path, schema))); body.append(add);
  } else renderScalar(parent, path, value, schema);
}
function renderScalar(parent, path, value, schema = {}, required = false) {
  if (path[0] === 'quest_nodes' && path.at(-1) === 'kind') schema = { enum: sceneCommands };
  const wrap = el('div', 'field'), label = el('label', '', human(path.at(-1) || 'Value') + (required ? ' *' : ''));
  if (path.length) label.append(button('×', () => remove(path), 'remove')); wrap.append(label); let input;
  const options = schema.enum || (schema.const !== undefined ? [schema.const] : null);
  if (options) { input = el('select'); for (const v of options) input.append(new Option(String(v), String(v))); input.value = String(value); input.onchange = () => set(path, options[input.selectedIndex]); }
  else if (typeof value === 'boolean') { input = el('input'); input.type = 'checkbox'; input.checked = value; input.onchange = () => set(path, input.checked); }
  else if (value === null) { input = el('select'); for (const t of ['null', 'string', 'number', 'boolean', 'object', 'array']) input.append(new Option(t, t)); input.onchange = () => set(path, ({ null: null, string: '', number: 0, boolean: false, object: {}, array: [] })[input.value], true); }
  else if (typeof value === 'number' || isRaw(value)) { input = el('input'); input.type = 'text'; input.inputMode = 'decimal'; input.value = isRaw(value) ? value.rawJSON : String(value); input.oninput = () => { try { const parsed = parse(input.value); if (typeof parsed !== 'number' && !isRaw(parsed)) throw Error('Enter a JSON number'); set(path, parsed); input.classList.remove('error'); input.removeAttribute('aria-invalid'); } catch { input.classList.add('error'); input.setAttribute('aria-invalid', 'true'); } }; }
  else { input = el(String(value).length > 100 || /text|description|caption|note/i.test(path.at(-1)) ? 'textarea' : 'input'); input.value = value ?? ''; input.oninput = () => set(path, input.value); }
  const id = `field-${++fieldSequence}`; input.id = id; input.setAttribute('aria-label', human(path.at(-1) || 'Value')); label.htmlFor = id; wrap.append(input);
  if (schema.description) wrap.append(el('small', '', schema.description));
  if (typeof value === 'string' && (value.startsWith('@') || value.startsWith('#') || /\.(scene|ent|app|mesh|wem|workspot)$/.test(value))) wrap.append(button('Find reference →', () => { state.referenceQuery = value; state.view = 'references'; renderDocument(); }, 'link-button'));
  if (['id', 'key'].includes(path.at(-1)) && path.length > 1 && typeof value === 'string') wrap.append(button('Rename references…', () => renameReference(path, value), 'link-button'));
  parent.append(wrap);
}
async function addField(path, schema) {
  const value = at(path);
  if (Array.isArray(value)) { if (path[0] === 'stages' && path.length === 1) return addBlock('time_gate'); checkpoint(); const entry = arrayEntry(path, schema); if (path.at(-1) === 'entities') { delete entry.template; entry.entity = ''; } if (path.at(-1) === 'devices') { delete entry.template; entry.buffer_id = Math.max(0, ...value.map(d => Number(d.buffer_id ?? 1))) + 1; } value.push(entry); changed(true); return; }
  const available = Object.keys(resolveSchema(schema).properties || {}).filter(k => !Object.hasOwn(value, k));
  const result = await ask('Add field', p => {
    const list = el('datalist'); list.id = 'field-options'; available.forEach(k => list.append(new Option(k, k))); p.append(list);
    const name = inputField(p, 'Field name'); name.setAttribute('list', list.id);
    const type = el('select'); for (const t of ['From contract', 'String', 'Number', 'Boolean', 'Object', 'Array', 'Null']) type.append(new Option(t, t)); p.append(type);
    return () => ({ name: name.value.trim(), type: type.value });
  }, 'Add');
  if (!result?.name) return; if (Object.hasOwn(value, result.name)) throw Error('This field already exists.');
  const v = result.type === 'From contract' ? defaultValue(resolveSchema(schema).properties?.[result.name], result.name) : ({ String: '', Number: 0, Boolean: false, Object: {}, Array: [], Null: null })[result.type];
  set([...path, result.name], v, true);
}
function moveItem(path, index, delta) { const array = at(path), target = index + delta; if (target < 0 || target >= array.length) return; checkpoint(); [array[index], array[target]] = [array[target], array[index]]; state.selection = []; changed(true); }
function renderSource(parent) {
  const note = el('p', 'muted', 'Edits update the same source document used by the structured views. Invalid JSON stays unsaved until corrected.'); parent.append(note);
  const input = el('textarea', 'source'); input.spellcheck = false; input.value = state.doc.text;
  input.oninput = () => { checkpoint(); state.doc.text = input.value; state.doc.invalid = false; try { if (state.doc.path.endsWith('.json')) state.doc.value = parse(input.value); } catch { state.doc.invalid = true; } state.doc.dirty = true; clearTimeout(state.saveTimer); if (!state.doc.invalid) state.saveTimer = setTimeout(() => save().catch(error), 1500); status(); if (state.doc.invalid) $('#save-status').textContent = 'Invalid JSON · not saved'; };
  parent.append(input);
}
function renderOverview(parent) {
  const value = state.doc.value; if (value === undefined) { parent.append(el('div', 'readme', state.doc.text)); return; }
  const card = el('div', 'card'); card.append(el('h2', '', nameOf(value)), el('p', 'muted', value.description || 'Authoring document'));
  const stats = el('div', 'stats'); for (const key of ['stages', 'sections', 'actors', 'spoken_lines', 'choice_lines', 'markers', 'triggers', 'entities', 'devices']) if (Array.isArray(value[key])) { const n = el('div'); n.append(el('strong', '', value[key].length), el('span', '', human(key))); stats.append(n); } card.append(stats); parent.append(card);
  if (kind(state.doc.path, value) === 'quest') { const row = el('div', 'toolbar'); for (const [text, command] of [['Validate quest', 'validate'], ['Inspect plan', 'plan'], ['Build resources', 'compile'], ['Run scenarios', 'scenarios']]) row.append(button(text, () => tool(command))); parent.append(row); }
  const fields = el('div', 'card'); fields.append(el('h3', '', 'Document details')); const head = { ...value }; for (const k of Object.keys(head)) if (typeof head[k] === 'object') delete head[k];
  for (const k of Object.keys(head)) renderScalar(fields, [k], value[k], schemaFor([k])); parent.append(fields);
  const collections = el('div', 'welcome-grid'); for (const [key, v] of Object.entries(value)) if (v && typeof v === 'object') { const b = button('', () => select([key])); b.append(el('b', '', human(key)), el('small', '', `${Array.isArray(v) ? v.length + ' entries' : Object.keys(v).length + ' fields'} · inspect and edit`)); collections.append(b); } parent.append(collections);
}
function renderCollection(parent, path, noun) {
  const values = at(path); if (!Array.isArray(values)) { parent.append(button(`+ Add ${human(path.at(-1))}`, () => set(path, [], true))); return; }
  const toolbar = el('div', 'toolbar'); toolbar.append(el('h3', '', `${human(path.at(-1))} · ${values.length}`), button(`+ ${noun}`, () => { if (path[0] === 'stages') return addBlock($('#editor .toolbar select')?.value || 'time_gate'); checkpoint(); values.push(path[0] === 'actors' ? { key: `actor_${values.length + 1}`, kind: 'community', community_ref: '', entry: '', appearance: 'default', lipsync: true } : { key: `new_${noun.toLowerCase()}_${values.length + 1}` }); changed(true); })); parent.append(toolbar);
  values.forEach((value, i) => { const row = el('div', 'list-row' + (pointer(state.selection) === pointer([...path, i]) ? ' selected' : '')); row.append(el('span', 'index', String(i + 1).padStart(2, '0')), el('span', 'grow', nameOf(value)), el('span', 'badge', value.kind || value.type || noun)); row.onclick = () => select([...path, i]); parent.append(row); });
}
function renderRouting(parent, path, stage) {
  const wrap = el('details', 'field-group'); wrap.open = true; wrap.append(el('summary', '', 'Routing'));
  const routes = stage.on || { success: '' };
  for (const [outcome, target] of Object.entries(routes)) {
    if (typeof target !== 'string') continue;
    const field = el('div', 'field'), label = el('label', '', human(outcome)), input = el('select'); input.append(new Option('Next block (sequential)', ''), new Option('Quest complete', '$end'));
    for (const s of state.doc.value.stages || []) input.append(new Option(s.id, s.id)); input.value = target;
    input.onchange = () => { checkpoint(); if (!input.value) { if (stage.on) { delete stage.on[outcome]; if (!Object.keys(stage.on).length) delete stage.on; } } else (stage.on ||= {})[outcome] = input.value; changed(true); }; label.append(input); field.append(label); wrap.append(field);
  }
  wrap.append(button('+ Outcome route', async () => { const name = await ask('Route an outcome', p => { const input = inputField(p, 'Outcome alias', 'success'); return () => input.value.trim(); }, 'Add route'); if (!name) return; checkpoint(); (stage.on ||= {})[name] = '$end'; changed(true); })); parent.append(wrap);
}
function renderSubtree(parent, path, title, fallback) {
  parent.append(el('h2', '', title)); if (at(path) === undefined) { parent.append(el('p', 'muted', `Add ${title.toLowerCase()} to the quest's composition.`), button(`+ ${title}`, () => { checkpoint(); let v = state.doc.value; for (const key of path.slice(0, -1)) v = v[key] ||= {}; v[path.at(-1)] = fallback; changed(true); })); }
  else renderFields(parent, path, at(path), schemaFor(path), 0, true);
}
function flowNodes() { return (state.doc.value.stages || []).map((stage, i) => ({ id: stage.id, label: authoring && !state.advanced ? authoring.stageName(stage) : stage.id, sub: stage.type, path: ['stages', i], value: stage })); }
function flowEdges(nodes) {
  const edges = []; nodes.forEach((n, i) => { const routes = n.value.on; if (routes && Object.keys(routes).length) { for (const [port, target] of Object.entries(routes)) { for (const t of Array.isArray(target) ? target : [target]) edges.push({ from: n.id, to: typeof t === 'string' ? t : t?.stage || t?.target, label: port }); } } else edges.push({ from: n.id, to: nodes[i + 1]?.id || '$end', label: 'success' }); }); return edges;
}
function renderFlow(parent) {
  const toolbar = el('div', 'toolbar'), types = el('select'); catalog.blocks.slice().sort((a, b) => a.type.localeCompare(b.type)).forEach(b => types.append(new Option(human(b.type), b.type)));
  toolbar.append(types, button('+ Add block', () => addBlock(types.value)), button('Routing', () => select(['stages'])), el('span', 'muted', 'Select a block to edit its fields and routes')); parent.append(toolbar);
  const nodes = flowNodes(), edges = flowEdges(nodes); if (edges.some(e => e.to === '$end')) nodes.push({ id: '$end', label: 'Quest complete', sub: 'End', path: [] });
  drawGraph(parent, nodes, edges);
  if (state.doc.value.parallel_groups) { const p = el('div', 'card'); p.append(el('h3', '', 'Parallel groups')); renderFields(p, ['parallel_groups'], state.doc.value.parallel_groups, schemaFor(['parallel_groups'])); parent.append(p); }
  const list = el('div'); list.style.marginTop = '20px'; renderCollection(list, ['stages'], 'Block'); parent.append(list);
}
async function addBlock(type) {
  const block = catalog.blocks.find(b => b.type === type), number = (state.doc.value.stages || []).length + 1;
  const result = await ask('Add ' + human(type), p => { const id = inputField(p, 'Block name', `${type}_${number}`); p.append(el('p', 'muted', 'Native node IDs are generated by the compiler. Required properties are exposed in the inspector.')); return () => id.value.trim(); }, 'Add block');
  if (!result) return; if (!/^[a-z][a-z0-9_]*$/.test(result)) throw Error('Use a lower-case name with letters, numbers and underscores.');
  if (state.doc.value.stages?.some(s => s.id === result)) throw Error('A block with this name already exists.');
  const stage = { id: result, type, status: 'ready', phase_resource: `mod\\${state.doc.value.id}\\phases\\${result}.questphase` };
  for (const [key, f] of Object.entries(block.fields || {})) if (f.required && !(key in stage)) stage[key] = defaultValue(f.schema, key);
  checkpoint(); state.doc.value.stages ||= []; state.doc.value.stages.push(stage); state.selection = ['stages', state.doc.value.stages.length - 1]; changed(true);
}
const NS = 'http://www.w3.org/2000/svg';
function svgEl(tag, attrs = {}, text) { const n = document.createElementNS(NS, tag); for (const [k, v] of Object.entries(attrs)) n.setAttribute(k, v); if (text !== undefined) n.textContent = text; return n; }
function drawGraph(parent, nodes, edges) {
  const columns = Math.max(1, Math.min(3, Math.floor((parent.clientWidth - 20) / 260)));
  const container = el('div', 'graph'), svg = svgEl('svg', { width: columns * 260 + 30, height: Math.max(390, Math.ceil(nodes.length / columns) * 130 + 60), role: 'img', 'aria-label': 'Authoring flow graph' });
  const positions = new Map(nodes.map((n, i) => [n.id, { x: 20 + i % columns * 260, y: 35 + Math.floor(i / columns) * 130 }]));
  const defs = svgEl('defs'), marker = svgEl('marker', { id: 'arrow', viewBox: '0 0 10 10', refX: 8, refY: 5, markerWidth: 6, markerHeight: 6, orient: 'auto-start-reverse' }); marker.append(svgEl('path', { d: 'M 0 0 L 10 5 L 0 10 z', fill: '#7b996a' })); defs.append(marker); svg.append(defs);
  for (const edge of edges) { const a = positions.get(edge.from), b = positions.get(edge.to); if (!a || !b) continue; const same = a.y === b.y; const x = a.x + 220, y = a.y + 30, bx = b.x, by = b.y + 30;
    svg.append(svgEl('path', { d: same && bx > x ? `M${x},${y} L${bx},${by}` : `M${a.x + 110},${a.y + 60} C${a.x + 110},${a.y + 100} ${b.x + 110},${b.y - 35} ${b.x + 110},${b.y}`, class: 'edge', 'marker-end': 'url(#arrow)' }));
    svg.append(svgEl('text', { x: same ? x + 4 : a.x + 115, y: same ? y - 7 : a.y + 79, class: 'edge-label' }, edge.label || ''));
  }
  for (const node of nodes) { const p = positions.get(node.id); const g = svgEl('g', { transform: `translate(${p.x},${p.y})`, class: 'node' + (pointer(node.path) === pointer(state.selection) ? ' selected' : ''), tabindex: 0, role: 'button', 'aria-label': node.label }); g.append(svgEl('rect', { width: 220, height: 62, rx: 5 }), svgEl('text', { x: 12, y: 26, 'font-size': 12 }, node.label.length > 29 ? node.label.slice(0, 27) + '…' : node.label), svgEl('text', { x: 12, y: 45, class: 'sub' }, node.sub)); g.onclick = () => select(node.path); g.onkeydown = e => { if (e.key === 'Enter') select(node.path); }; svg.append(g); }
  container.append(svg); parent.append(container);
}
function sceneNodes() { const d = state.doc.value; return [...['start_nodes', 'sections', 'choices', 'quest_nodes', 'xor_nodes', 'end_nodes'].flatMap(type => (d[type] || []).map((n, i) => ({ id: n.key || n.name, label: n.key || n.name || `${type}_${i}`, sub: n.kind || human(type), path: [type, i], value: n }))), ...['start_node', 'end_node'].filter(k => d[k]).map(k => ({ id: d[k].key, label: d[k].key, sub: human(k), path: [k], value: d[k] }))]; }
function renderSceneGraph(parent) {
  const toolbar = el('div', 'toolbar'), type = el('select'); ['sections', 'choices', 'quest_nodes', 'start_nodes', 'end_nodes', 'xor_nodes'].forEach(t => type.append(new Option(human(t), t)));
  toolbar.append(type, button('+ Add node', () => { const t = type.value; checkpoint(); const values = state.doc.value[t] ||= []; const key = `${t.replace(/s$/, '')}_${values.length + 1}`; values.push(({ sections: { key, lines: [], on_end: [] }, choices: { key, options: [] }, quest_nodes: { key, kind: 'realtime_delay', duration_ms: 1000, on_out: [] }, start_nodes: { key, on_start: [] }, end_nodes: { key }, xor_nodes: { key, on_out: [] } })[t]); state.selection = [t, values.length - 1]; changed(true); }), button('Audit scene', () => tool('scene-audit')), button('Open dialogue', async () => { if (typeof state.doc.value.manifest === 'string') { const linked = await linkedDocument(state.doc.value.manifest); await openFile(linked.path); } else { select(['manifest']); } })); parent.append(toolbar);
  const nodes = sceneNodes(), edges = [];
  for (const n of nodes) { for (const [k, v] of Object.entries(n.value)) if (k.startsWith('on_') && Array.isArray(v)) for (const t of v) edges.push({ from: n.id, to: typeof t === 'string' ? t : t.node, label: k.slice(3) }); for (const o of n.value.options || []) edges.push({ from: n.id, to: o.target, label: o.caption?.slice(0, 22) || o.choice_key }); }
  drawGraph(parent, nodes, edges);
}
async function linkedDocument(relative) {
  const parts = state.doc.path.split('/'); parts.pop(); for (const part of relative.replaceAll('\\', '/').split('/')) { if (part === '..') parts.pop(); else if (part !== '.') parts.push(part); }
  const path = parts.join('/'), key = `${state.doc.root}:${path}`; const cached = state.docs.get(key); if (cached) return { path, value: cached.value };
  const response = await api('read', { root: state.doc.root, path }); return { path, value: parse(response.text) };
}
async function renderTimeline(parent) {
  parent.append(el('p', 'muted', 'Sections share a time scale. Gaps and tail padding use the compiler defaults unless overridden. Branches are separate; select a section to edit its timing and performance.'));
  let lines = []; try { const manifest = state.doc.value.manifest; lines = (typeof manifest === 'string' ? (await linkedDocument(manifest)).value : manifest)?.spoken_lines || []; } catch (e) { parent.append(el('p', 'error', e.message)); }
  if (!parent.isConnected) return;
  const lineMap = new Map(lines.map(l => [l.key, l]));
  const rows = (state.doc.value.sections || []).map((section, i) => {
    const gap = Number(section.line_gap_ms ?? state.doc.value.line_gap_ms ?? 250), tail = Number(section.tail_padding_ms ?? state.doc.value.section_tail_padding_ms ?? 400); let start = 0;
    const events = (section.lines || []).map(key => { const line = lineMap.get(key), duration = Number(line?.duration_ms ?? state.doc.value.default_line_duration_ms ?? 2500); const e = { label: `${line?.speaker || key} · ${(duration / 1000).toFixed(1)}s`, text: line?.text || key, start, duration }; start += duration + gap; return e; });
    let duration = Number(section.rid_duration_ms ?? section.section_duration_ms ?? (events.length ? start - gap + tail : 1000));
    for (const [kind, values] of [['Animation', section.rid_events || []], ['Gaze', section.look_at_events || []], ['Camera', [section.rid_camera || section.camera_event].filter(Boolean)]]) for (const e of values) { const event = { label: `${kind} · ${e.actor || ''}`, text: pretty(e), start: Number(e.start_time_ms || 0), duration: Number(e.duration_ms || 0), kind }; events.push(event); if (section.rid_duration_ms === undefined && section.section_duration_ms === undefined) duration = Math.max(duration, event.start + event.duration); }
    return { section, i, events, duration, gap, tail };
  });
  const maxDuration = Math.max(1000, ...rows.map(r => r.duration)), pixels = .035, width = Math.max(400, maxDuration * pixels), scroll = el('div', 'timeline-scroll');
  const axis = el('div', 'timeline-track'); axis.append(el('span', 'timeline-label', 'Seconds')); const ticks = el('div', 'timeline-lane'); ticks.style.width = `${width}px`; for (let time = 0; time <= maxDuration; time += 2000) { const tick = el('span', 'timeline-tick', String(time / 1000)); tick.style.left = `${time * pixels}px`; ticks.append(tick); } axis.append(ticks); scroll.append(axis);
  for (const r of rows) { const lanes = [...new Set(['Dialogue', ...r.events.filter(e => e.kind).map(e => e.kind)])]; for (const [index, kind] of lanes.entries()) { const row = el('div', 'timeline-track'); row.append(button(index === 0 ? `${r.section.key} · ${(r.duration / 1000).toFixed(1)}s` : kind, () => select(['sections', r.i]), 'timeline-label link-button')); const lane = el('div', 'timeline-lane'); lane.style.width = `${width}px`; const end = el('div', 'timeline-end'); end.style.left = `${r.duration * pixels}px`; lane.append(end);
      for (const event of r.events.filter(e => (e.kind || 'Dialogue') === kind)) { const bar = button(event.label, () => select(['sections', r.i]), 'timeline-event'); bar.title = event.text; bar.style.position = 'absolute'; bar.style.left = `${event.start * pixels}px`; bar.style.width = `${Math.max(4, event.duration * pixels)}px`; bar.style.minWidth = '0'; if (event.kind) bar.style.background = '#293c50'; lane.append(bar); } row.append(lane); scroll.append(row); } }
  parent.append(scroll);
  const performances = ['performance', 'performances', 'events'].filter(k => state.doc.value[k]);
  for (const k of performances) {
    if (state.advanced) renderFields(parent, [k], at([k]), {}, 0, true);
    else { const details = el('details', 'stage-advanced'); details.append(el('summary', '', `Advanced ${human(k).toLowerCase()} settings`)); renderFields(details, [k], at([k]), {}, 0, true); parent.append(details); }
  }
  if (!state.doc.value.sections?.length) parent.append(el('div', 'empty', 'Add sections in the scene graph to create a timeline.'));
}
function renderDialogue(parent) {
  const toolbar = el('div', 'toolbar'); toolbar.append(button('+ Spoken line', () => { checkpoint(); const lines = state.doc.value.spoken_lines ||= []; lines.push({ key: `line_${lines.length + 1}`, speaker: '', text: '', duration_ms: 0 }); changed(true); }), button('+ Choice line', () => { checkpoint(); const lines = state.doc.value.choice_lines ||= []; lines.push({ key: `choice_${lines.length + 1}`, text: '' }); changed(true); })); parent.append(toolbar);
  for (const collection of ['spoken_lines', 'choice_lines']) { if (!state.doc.value[collection]) continue; parent.append(el('h3', '', human(collection))); const table = el('table', 'table'), head = el('tr'); for (const name of ['Key', 'Speaker', 'Text', 'Duration', '']) head.append(el('th', '', name)); table.append(head);
    state.doc.value[collection].forEach((line, i) => { const row = el('tr'); for (const key of ['key', 'speaker', 'text', 'duration_ms']) { const td = el('td'), input = el(key === 'text' ? 'textarea' : 'input'); input.value = line[key] ?? ''; input.setAttribute('aria-label', `${line.key || 'Line ' + (i + 1)} ${human(key)}`); if (key === 'duration_ms') { input.type = 'number'; input.min = '0'; } input.oninput = () => set([collection, i, key], key === 'duration_ms' ? Number(input.value) : input.value); td.append(input); row.append(td); } const td = el('td'); td.append(button('Details', () => select([collection, i]))); row.append(td); table.append(row); }); parent.append(table);
  }
}
function allReferences(value, path = [], result = []) { if (typeof value === 'string') result.push({ path, value }); else if (value && typeof value === 'object' && !isRaw(value)) for (const [k, v] of Object.entries(value)) allReferences(v, [...path, k], result); return result; }
function referencesTo(query) { return allReferences(state.doc.value).filter(r => r.value.includes(query)); }
async function renameReference(path, old) {
  const result = await ask('Rename authoring reference', p => { const input = inputField(p, 'New name', old); p.append(el('p', 'muted', 'Updates exact named references in this document. Cross-file matches are listed for review; their files are not silently rewritten.')); return () => input.value.trim(); }, 'Rename');
  if (!result || result === old) return; checkpoint(); let count = 0; for (const r of allReferences(state.doc.value)) if (r.value === old) { at(r.path.slice(0, -1))[r.path.at(-1)] = result; count++; } changed(true); toast(`Updated ${count} exact reference(s). Check References for other files.`);
}
function renderReferences(parent) {
  const input = el('input'); input.type = 'search'; input.placeholder = 'Find a named reference, record, resource path or fact…'; input.value = state.referenceQuery || (state.selection.length ? String(nameOf(at(state.selection))) : '');
  const results = el('div'); parent.append(input, results);
  const run = async () => { const query = input.value; state.referenceQuery = query; results.replaceChildren(); if (!query) { results.append(el('p', 'muted', 'Enter a reference to find uses in the workspace.')); return; }
    const matches = referencesTo(query); for (const r of matches) { const card = el('div', 'list-row'); card.append(button(pointer(r.path), () => select(r.path), 'link-button'), el('span', 'reference', r.value)); results.append(card); }
    const hint = el('p', 'muted', 'Searching authoring files…'); results.append(hint); let count = 0;
    for (const file of state.files.find(r => r.id === state.root)?.files || []) { if (input.value !== query || !results.isConnected) return; if (file.path === state.doc.path || !file.path.endsWith('.json') || file.size > 300000) continue;
      try { const doc = state.docs.get(`${state.root}:${file.path}`); const text = doc?.text || (await api('read', { root: state.root, path: file.path })).text; if (text.includes(query)) { count++; results.append(button(file.path, () => openFile(file.path), 'link-button'), el('br')); } } catch { /* An unreadable file is still visible in the project browser. */ }
    } hint.textContent = `${matches.length} matches in this document · ${count} other files`;
  };
  let timer; input.oninput = () => { clearTimeout(timer); timer = setTimeout(() => run().catch(error), 350); }; run().catch(error);
}

function worldPlacements() {
  const d = state.doc.value, placements = [], anchors = new Map();
  const origin = d.origin || { x: 0, y: 0, z: 0, yaw: d.yaw || 0 }; const base = Array.isArray(origin) ? { x: origin[0], y: origin[1], z: origin[2], yaw: d.yaw || 0 } : origin;
  anchors.set('origin', { x: Number(base.x || 0), y: Number(base.y || 0), z: Number(base.z || 0), yaw: Number(base.yaw || 0) });
  const position = (v = {}, defaultYaw) => {
    if (Array.isArray(v)) return { x: Number(v[0]), y: Number(v[1]), z: Number(v[2]), yaw: defaultYaw || 0 };
    if (['x', 'y', 'z'].every(k => k in v)) return { x: Number(v.x), y: Number(v.y), z: Number(v.z), yaw: defaultYaw || anchors.get('origin').yaw };
    const anchor = anchors.get(v.from || 'origin'); if (!anchor) return null;
    const yaw = typeof v.yaw === 'number' ? v.yaw : (anchors.get(v.yaw)?.yaw ?? defaultYaw ?? anchor.yaw), angle = yaw * Math.PI / 180;
    const forward = Number(v.forward || 0) + Math.cos(Number(v.bearing || 0) * Math.PI / 180) * Number(v.distance || 0), right = Number(v.right || 0) + Math.sin(Number(v.bearing || 0) * Math.PI / 180) * Number(v.distance || 0);
    return { x: anchor.x + Math.cos(angle) * forward - Math.sin(angle) * right, y: anchor.y + Math.sin(angle) * forward + Math.cos(angle) * right, z: anchor.z + Number(v.up ?? v.z_offset ?? 0), yaw };
  };
  function add(value, path, type) { const p = position(value.position || {}); if (!p) { placements.push({ value, path, type, unresolved: true }); return; } const moveYaw = p.yaw; p.yaw = typeof value.yaw === 'number' ? value.yaw : anchors.get(value.yaw)?.yaw ?? p.yaw; const entry = { value, path, type, ...p, moveYaw }; placements.push(entry); for (const ref of [value.ref, value.ref?.replace(/^#/, ''), value.ref?.startsWith('$/') ? value.ref : `${d.prefab_root}/${value.ref?.replace(/^#/, '')}`]) if (ref) anchors.set(ref, p); }
  for (const type of ['markers', 'triggers', 'entities', 'loot_containers', 'crowd_null_areas', 'devices']) for (const [i, v] of (d[type] || []).entries()) add(v, [type, i], type);
  const communities = [...(d.community ? [{ value: d.community, path: ['community'] }] : []), ...(d.communities || []).map((value, i) => ({ value, path: ['communities', i] }))];
  for (const { value: community, path } of communities) {
    const entries = community.entries?.map((entry, i) => ({ entry: { ...community, ...entry }, source: entry, path: [...path, 'entries', i] })) || [{ entry: community, source: community, path }];
    for (const e of entries) {
      const spots = e.source.spots || e.entry.spots;
      if (spots) spots.forEach((spot, i) => add({ ...community.spot, ...spot }, [...(e.source.spots ? e.path : path), 'spots', i], 'community'));
      else if (e.source.spot || community.spot) add({ ...community.spot, ...e.source.spot }, [...(e.source.spot ? e.path : path), 'spot'], 'community');
    }
    add(community, path, 'community area');
  }
  for (const type of ['ai_spots', 'splines', 'patrol_splines']) for (const [i, v] of (d[type] || []).entries()) add(v, [type, i], type);
  return { placements, origin: anchors.get('origin'), anchors };
}
function renderWorld(parent) {
  const toolbar = el('div', 'toolbar'), types = el('select'); ['Marker', 'Trigger', 'Entity', 'Device', 'Loot container'].forEach(t => types.append(new Option(t, t)));
  toolbar.append(types, button('+ Place', () => addPlacement(types.value)), button('Origin / anchor', () => select(['origin'])), button('Use captured position', importPosition), button('Fit', () => { state.pan = { x: 0, y: 0 }; state.scale = 0; renderDocument(); }), button('Inspect world plan', () => tool('world-plan'))); parent.append(toolbar);
  parent.append(el('p', 'muted', 'Local top-down placement · drag to move · scroll to zoom. Coordinates use the compiler’s anchor system. Verify terrain, collisions and final placement in game.'));
  const { placements, origin, anchors } = worldPlacements();
  const resolved = placements.filter(p => !p.unresolved); if (!state.scale) { const max = Math.max(5, ...resolved.map(p => Math.max(Math.abs(p.x - origin.x), Math.abs(p.y - origin.y)) + Number(p.value.outline?.radius || 0))); state.scale = Math.min(25, 185 / max); }
  const container = el('div', 'graph'), svg = svgEl('svg', { viewBox: '-400 -215 800 430', class: 'world-svg', role: 'img', 'aria-label': 'Local quest placement editor' });
  const screen = p => ({ x: (p.x - origin.x) * state.scale + state.pan.x, y: -(p.y - origin.y) * state.scale + state.pan.y });
  const step = 10 ** Math.floor(Math.log10(70 / state.scale));
  for (let i = -100; i < 100; i++) { const n = i * step * state.scale; if (Math.abs(n) < 430) { svg.append(svgEl('line', { x1: n + state.pan.x, y1: -215, x2: n + state.pan.x, y2: 215, stroke: '#2b353c', 'stroke-width': .5 }), svgEl('line', { x1: -400, y1: n + state.pan.y, x2: 400, y2: n + state.pan.y, stroke: '#2b353c', 'stroke-width': .5 })); } }
  svg.append(svgEl('path', { d: `M${state.pan.x - 10},${state.pan.y} h20 M${state.pan.x},${state.pan.y - 10} v20`, stroke: '#efb77f' }), svgEl('text', { x: state.pan.x + 10, y: state.pan.y - 10 }, 'origin'));
  for (const p of resolved) { const q = screen(p), selected = pointer(p.path) === pointer(state.selection), g = svgEl('g', { transform: `translate(${q.x},${q.y})`, class: `placed ${p.type === 'triggers' ? 'trigger' : ''} ${selected ? 'selected' : ''}`, tabindex: 0, role: 'button', 'aria-label': nameOf(p.value) });
    const outline = p.value.outline || p.value.vehicle_forbidden_outline || p.value.ambient_outline;
    if (outline) {
      let points = outline.points;
      if (!points && ['circle', 'disc', 'regular_polygon'].includes(outline.type)) { const count = Math.min(512, Number(outline.points_count ?? outline.segments ?? 12)), radius = Number(outline.radius ?? 1); points = Array.from({ length: count }, (_, i) => [Math.cos(i * Math.PI * 2 / count) * radius, Math.sin(i * Math.PI * 2 / count) * radius]); }
      if (!points) { const w = Number(outline.width ?? outline.size ?? 2) / 2, d = Number(outline.depth ?? outline.length ?? outline.size ?? w * 2) / 2; points = [[-w, -d], [w, -d], [w, d], [-w, d]]; }
      const pts = points.map(v => Array.isArray(v) ? `${Number(v[0]) * state.scale},${-Number(v[1]) * state.scale}` : `${Number(v.x) * state.scale},${-Number(v.y) * state.scale}`).join(' '); g.append(svgEl('polygon', { points: pts, transform: `rotate(${-p.yaw})`, fill: '#83b9e210', stroke: '#83b9e2', 'pointer-events': 'none' }));
    }
    g.append(svgEl('circle', { r: 5, fill: selected ? '#fff' : '#c4ef91' }), svgEl('path', { d: `M0,0 l${Math.cos(p.yaw * Math.PI / 180) * 18},${-Math.sin(p.yaw * Math.PI / 180) * 18}`, stroke: '#c4ef91' }));
    if (selected) g.append(svgEl('text', { x: 9, y: -9 }, nameOf(p.value))); const title = svgEl('title', {}, nameOf(p.value)); g.append(title);
    g.onpointerdown = e => { e.stopPropagation(); svg.setPointerCapture(e.pointerId); const start = { x: e.clientX, y: e.clientY }; let moved = false; const rect = svg.getBoundingClientRect();
      svg.onpointermove = event => { const dx = (event.clientX - start.x) * 800 / rect.width, dy = (event.clientY - start.y) * 430 / rect.height; if (Math.abs(dx) + Math.abs(dy) > 2) moved = true; g.setAttribute('transform', `translate(${q.x + dx},${q.y + dy})`); };
      svg.onpointerup = event => { svg.onpointermove = null; svg.onpointerup = null; if (moved) { const x = p.x + (event.clientX - start.x) * 800 / rect.width / state.scale, y = p.y - (event.clientY - start.y) * 430 / rect.height / state.scale;
          const old = p.value.position, anchor = anchors.get(old?.from || 'origin');
          if (old && !Array.isArray(old) && !('x' in old) && anchor) { const angle = p.moveYaw * Math.PI / 180, dx = x - anchor.x, dy = y - anchor.y; const next = { ...old, forward: Math.round((Math.cos(angle) * dx + Math.sin(angle) * dy) * 1000) / 1000, right: Math.round((-Math.sin(angle) * dx + Math.cos(angle) * dy) * 1000) / 1000 }; delete next.distance; delete next.bearing; set([...p.path, 'position'], next); }
          else set([...p.path, 'position'], { x: Math.round(x * 1000) / 1000, y: Math.round(y * 1000) / 1000, z: p.z });
        } state.selection = p.path; document.body.classList.add('inspecting'); renderDocument(); };
    }; g.onkeydown = e => { if (e.key === 'Enter') select(p.path); }; svg.append(g);
  }
  svg.onwheel = e => { e.preventDefault(); state.scale = Math.max(.05, Math.min(200, state.scale * (e.deltaY < 0 ? 1.2 : 1 / 1.2))); renderDocument(); };
  svg.onpointerdown = e => { const start = { x: e.clientX, y: e.clientY, ...state.pan }; const rect = svg.getBoundingClientRect(); svg.setPointerCapture(e.pointerId); svg.onpointermove = event => { state.pan = { x: start.x + (event.clientX - e.clientX) * 800 / rect.width, y: start.y + (event.clientY - e.clientY) * 430 / rect.height }; }; svg.onpointerup = () => { svg.onpointermove = null; svg.onpointerup = null; renderDocument(); }; };
  container.append(svg); parent.append(container);
  parent.append(el('p', 'muted', `Origin: ${origin.x.toFixed(3)}, ${origin.y.toFixed(3)}, ${origin.z.toFixed(3)} · Grid ${step} m · ${resolved.length} placements`));
  for (const p of placements.filter(p => p.unresolved)) parent.append(button(`Unresolved anchor: ${nameOf(p.value)} · inspect`, () => select(p.path), 'danger'));
  const table = el('table', 'table'), head = el('tr'); ['Placement', 'Kind', 'X', 'Y', 'Z'].forEach(t => head.append(el('th', '', t))); table.append(head);
  for (const p of resolved) { const row = el('tr'), first = el('td'); first.append(button(nameOf(p.value), () => select(p.path), 'link-button')); row.append(first, el('td', '', p.type), el('td', '', p.x.toFixed(3)), el('td', '', p.y.toFixed(3)), el('td', '', p.z.toFixed(3))); table.append(row); } parent.append(table);
}
async function addPlacement(type) {
  const name = await ask(`Place ${type.toLowerCase()}`, p => { const input = inputField(p, 'Named reference', `#${state.doc.value.name || 'quest'}_${type.toLowerCase()}_new`); return () => input.value.trim(); }, 'Place'); if (!name) return;
  const key = ({ Marker: 'markers', Trigger: 'triggers', Entity: 'entities', Device: 'devices', 'Loot container': 'loot_containers' })[type], value = { ref: name.replaceAll(' ', '_'), position: { from: 'origin', forward: 0, right: 0, up: 0 }, yaw: 'origin' };
  if (type === 'Trigger') { value.outline = { type: 'circle', radius: 3, height: 3, segments: 12 }; value.notifiers = ['quest']; }
  if (type === 'Entity') { value.entity = ''; value.appearance = 'default'; }
  if (type === 'Loot container') { value.entity_template = 'base\\gameplay\\loot\\containers\\weapon_cases\\weapon_case_small.ent'; value.appearance = 'default'; value.items = [{ id: 'Items.Preset_Katana_Default', quantity: 1, chance: 1 }]; }
  if (type === 'Device') { value.kind = 'access_point'; value.buffer_id = Math.max(0, ...(state.doc.value.devices || []).map(d => Number(d.buffer_id ?? 1))) + 1; }
  checkpoint(); const values = state.doc.value[key] ||= []; values.push(value); state.selection = [key, values.length - 1]; changed(true);
}
async function importPosition() {
  const capture = await ask('Use a captured game position', p => { p.append(el('p', 'muted', 'Paste a saved position with x, y, z and optional yaw. The quest placement group will use it as its origin.')); const text = el('textarea', 'source'); text.placeholder = '{"x": -406.1, "y": 724.7, "z": 127, "yaw": -40.45}'; p.append(text); return () => text.value; }, 'Set origin');
  if (!capture) return; const source = parse(capture), p = source.position || source.camera?.position || source;
  const origin = { x: Number(p.x ?? p.X), y: Number(p.y ?? p.Y), z: Number(p.z ?? p.Z), yaw: Number(source.yaw ?? p.yaw ?? source.camera?.yaw ?? 0) };
  if (Object.values(origin).some(n => !Number.isFinite(n))) throw Error('A captured position must contain finite x, y and z coordinates.');
  set(['origin'], origin, true);
}

function inferArgs(command) {
  const d = state.doc, args = {}, timestamp = new Date().toISOString().replace(/[:.]/g, '-'); if (!d) return args;
  const filename = d.path, output = `work/builds/${d.value?.id || d.value?.name || 'quest'}-${timestamp}`;
  for (const a of command.args) {
    if (a.id === 'out') args[a.id] = output;
    if (['spec', 'manifest', 'profile', 'resource'].includes(a.id)) args[a.id] = filename;
    if (['root', 'project'].includes(a.id)) args[a.id] = '.';
    if (a.id === 'game') args[a.id] = state.settings.game || '';
    if (a.id === 'schema') args[a.id] = state.settings.schema || '';
    if (a.id === 'name' && d.value?.id) args[a.id] = d.value.id;
    if (a.id === 'inputs' && command.name === 'compile') args[a.id] = '';
  }
  if (command.name === 'graph' || command.name === 'validate' || command.name === 'plan' || command.name === 'expand' || command.name === 'scene-audit' || command.name === 'world-plan' || command.name === 'catalog') delete args.out;
  return args;
}
function tool(command) { state.command = command; state.toolArgs = null; state.bottom = 'tools'; expandBottom(); renderBottom(); }
function expandBottom() { $('#bottom').hidden = false; document.body.classList.add('has-bottom'); $('#bottom').classList.add('expanded'); }
function renderJobOutput(parent, job) {
  let value; try { value = parse(job.stdout); } catch { /* Text commands have no structured report. */ }
  if (value && typeof value === 'object') {
    const summary = el('div', 'toolbar');
    if (value.valid !== undefined) summary.append(el('span', `badge ${value.valid ? 'good' : 'bad'}`, value.valid ? 'Validation passed' : 'Validation failed'));
    if (value.kind) summary.append(el('span', 'badge', value.kind));
    for (const name of ['resources', 'actors', 'events', 'diagnostics']) if (value[name]) summary.append(el('span', 'badge', `${Object.keys(value[name]).length} ${name}`));
    if (value.native_serialized !== undefined) summary.append(el('span', 'badge', value.native_serialized ? 'Native resources generated' : 'Resource documents'));
    if (value.output) summary.append(el('span', 'muted', String(value.output)));
    parent.append(summary);
    if (value.valid === false && !state.advanced) {
      for (const issue of state.diagnostics) {
        const row = el('div', 'story-notice'); row.append(el('span', '', issue.message), button('Review issue', () => revealDiagnostic(issue))); parent.append(row);
      }
    }
  }
  const details = el('details'); details.open = Boolean(state.advanced && job.status === 'failed'); details.append(el('summary', 'muted', 'Compiler output')); details.append(el('pre', 'log', [job.stdout, job.stderr, job.error].filter(Boolean).join('\n'))); parent.append(details);
}
async function revealDiagnostic(d) {
  if (d.file && (d.file !== state.doc?.path || d.root !== state.doc?.root)) await openFile(d.file, d.root);
  state.selection = unpointer(d.path || '');
  if (authoring) authoring.revealIssue(state.selection);
  else { renderInspector(); document.body.classList.add('inspecting'); }
}
function renderBottom() {
  document.querySelectorAll('[data-bottom]').forEach(b => b.classList.toggle('active', b.dataset.bottom === state.bottom));
  const parent = $('#bottom-content'); parent.replaceChildren(); $('#job-count').textContent = state.jobs.filter(j => j.status === 'running').length; $('#diagnostic-count').textContent = state.diagnostics.length;
  if (state.bottom === 'tools') { renderTools(parent); return; }
  if (state.bottom === 'diagnostics') {
    if (!state.diagnostics.length) parent.append(el('span', 'muted', 'No diagnostics recorded. Validate the current document to check it.'));
    for (const d of state.diagnostics) { const row = button('', () => revealDiagnostic(d), 'diagnostic'); row.append(el('code', '', d.path || d.code || ''), el('span', '', d.message || pretty(d))); parent.append(row); } return;
  }
  if (!state.jobs.length) parent.append(el('span', 'muted', 'No jobs running. Build and inspection results will appear here.'));
  for (const job of state.jobs.toReversed()) { const row = el('div', 'job-row'); row.append(el('span', `badge ${job.status === 'succeeded' ? 'good' : job.status === 'failed' ? 'bad' : ''}`, job.status), el('strong', '', human(job.command)), el('code', '', job.args.join(' '))); row.onclick = () => { state.selectedJob = state.selectedJob === job.id ? null : job.id; renderBottom(); }; parent.append(row);
    if (state.selectedJob === job.id) { renderJobOutput(parent, job); const actions = el('div', 'toolbar'); actions.append(button('Repeat with options', () => { state.command = job.command; state.toolArgs = commandArgsFromJob(job); state.bottom = 'tools'; renderBottom(); })); if (job.status === 'running') actions.append(button('Cancel job', async () => { await api('jobs/cancel', { id: job.id }); toast('Cancellation requested'); }, 'danger')); if (job.status === 'succeeded') actions.append(el('span', 'muted', 'Native checks and gameplay evidence are reported separately.')); parent.append(actions); }
  }
}
function commandArgsFromJob(job) { const spec = bootstrap.commands.find(c => c.name === job.command), values = {}; let pos = 0; for (let i = 0; i < job.args.length; i++) { const s = job.args[i]; if (s.startsWith('--')) { const arg = spec.args.find(a => a.long === s.slice(2)); if (arg) values[arg.id] = arg.flag ? true : job.args[++i]; } else { const a = spec.args.filter(a => a.positional)[pos++]; if (a) values[a.id] = s; } } return values; }
function renderTools(parent) {
  const toolbar = el('div', 'toolbar'), select = el('select'); for (const command of bootstrap.commands) select.append(new Option(human(command.name), command.name)); select.value = state.command; select.onchange = () => { state.command = select.value; state.toolArgs = null; renderBottom(); }; toolbar.append(select); parent.append(toolbar);
  const command = bootstrap.commands.find(c => c.name === state.command) || bootstrap.commands[0]; parent.append(el('p', 'muted', command.about));
  const values = state.toolArgs ||= inferArgs(command), form = el('div', 'tool-form');
  for (const arg of command.args) { const field = el('div', 'field'), label = el('label', '', human(arg.id) + (arg.required ? ' *' : '')), input = el(arg.multiple ? 'textarea' : 'input'); input.id = 'arg-' + arg.id; label.htmlFor = input.id;
    if (arg.flag) { input.type = 'checkbox'; input.checked = values[arg.id] === true; input.onchange = () => values[arg.id] = input.checked; }
    else { input.value = values[arg.id] ?? arg.defaults?.join('\n') ?? ''; values[arg.id] = input.value; input.placeholder = arg.multiple ? 'One value per line' : arg.id; input.oninput = () => values[arg.id] = input.value; }
    field.append(label, input); if (arg.help) field.append(el('small', '', arg.help)); form.append(field);
  }
  parent.append(form); const actions = el('div', 'toolbar'); actions.append(button('Run ' + human(command.name), async () => {
    await flush(); const args = []; for (const a of command.args) { const v = values[a.id]; if (a.flag) { if (v && a.long) args.push('--' + a.long); } else if (v !== undefined && String(v).trim()) { if (a.long) args.push('--' + a.long); args.push(...(a.multiple ? String(v).split('\n').map(s => s.trim()).filter(Boolean) : [String(v)])); } else if (a.required) throw Error(`${human(a.id)} is required.`); }
    if (['install', 'rollback', 'replace-build'].includes(command.name)) { const approved = await ask(human(command.name), p => { p.append(el('p', 'muted', 'Review the exact operation before changing the installation or published build.'), el('pre', 'log', `${command.name}\n${args.join('\n')}`)); return () => true; }, 'Run operation'); if (!approved) return; }
    if ('game' in values || 'schema' in values) { if ('game' in values) state.settings.game = values.game || ''; if ('schema' in values) state.settings.schema = values.schema || ''; localStorage.setItem('quest-workbench-settings', JSON.stringify(state.settings)); }
    await startJob(command.name, args);
  }, 'primary')); parent.append(actions);
}
async function startJob(command, args) { const result = await api('jobs', { root: state.root, command, args }); state.jobSources ||= {}; state.jobSources[result.id] = { root: state.doc?.root, file: state.doc?.path }; state.selectedJob = result.id; state.bottom = 'activity'; await pollJobs(); toast(`${human(command)} started`); }
async function validate() { if (!state.doc) return; await flush(); const type = kind(state.doc.path, state.doc.value); const command = ({ quest: 'validate', world: 'world-plan', scene: 'scene-audit', package: 'package' })[type]; if (!command) { toast('Saved valid JSON. Use Build & tools for document-specific checks.'); return; } await startJob(command, type === 'package' ? [state.doc.path, '--plan'] : [state.doc.path]); expandBottom(); }
async function pollJobs() {
  const jobs = await api('jobs'); const previous = new Map(state.jobs.map(j => [j.id, j.status])); state.jobs = jobs;
  for (const job of jobs) if (job.status !== 'running' && previous.get(job.id) === 'running') {
    let result; try { result = parse(job.stdout); } catch { /* Non-JSON commands retain their complete text output. */ }
    const source = state.jobSources?.[job.id] || {};
    if (job.command === 'validate') state.diagnostics = (result?.diagnostics || []).map(d => authoring ? authoring.describeIssue({ ...d, ...source }) : { ...d, ...source });
    if (job.status === 'failed' && !result?.diagnostics?.length) state.diagnostics.push({ code: job.command, message: job.stderr || job.error || 'Command failed', ...source });
    if (job.command === 'validate' && result?.valid) toast('Quest validation passed');
  }
  if (state.bottom !== 'tools') renderBottom(); else { $('#job-count').textContent = jobs.filter(j => j.status === 'running').length; }
  authoring?.syncStatus();
}
async function newFile() {
  const result = await ask('Create authoring document', p => {
    const type = el('select'); for (const t of ['Quest', 'Scene', 'Dialogue', 'World placement', 'JSON document', 'Markdown']) type.append(new Option(t, t)); p.append(type);
    const path = inputField(p, 'Workspace-relative path', 'quests/new_quest/quest.json');
    p.append(el('p', 'muted', 'Directories are created inside the workspace. No existing document is overwritten.'));
    return () => ({ type: type.value, path: path.value.trim() });
  }, 'Create'); if (!result) return;
  await flush(); const filename = result.path, id = filename.split('/').filter(Boolean).at(-2)?.replaceAll('-', '_') || 'new_quest';
  const value = ({ Quest: { schema_version: 1, id, title: 'New quest', stages: [], composition: {} }, Scene: { name: id + '_scene', manifest: '../script/dialogue.json', archive_path: `mod\\${id}\\scenes\\${id}.scene`, actors: [], sections: [], choices: [], start_nodes: [], end_nodes: [], entry_points: [], exit_points: [] }, Dialogue: { sequence_id: id, spoken_lines: [], choice_lines: [] }, 'World placement': { name: id, prefab_root: `$/mod/${id}/#${id}_placement`, block_path: `mod\\${id}\\world\\${id}.streamingblock`, quest_sector_path: `mod\\${id}\\world\\${id}.streamingsector`, origin: { x: 0, y: 0, z: 0, yaw: 0 }, markers: [], triggers: [], entities: [] }, 'JSON document': {} })[result.type];
  const text = result.type === 'Markdown' ? '# New document\n' : pretty(value) + '\n';
  await api('save', { root: state.root, path: filename, text, hash: null }); await refresh(); await openFile(filename, state.root, true);
}
async function showHistory() {
  const d = state.doc, history = await api('history', { root: d.root, path: d.path });
  if (!history.versions.length) { toast('No earlier saved versions for this document.'); return; }
  const version = await ask('Restore a saved version', p => { const select = el('select'); for (const id of history.versions) select.append(new Option(new Date(Number(BigInt(id) / 1000000n)).toLocaleString(), id)); p.append(el('p', 'muted', 'Restoring creates an editable change. The current saved version remains in history.'), select); return () => select.value; }, 'Restore');
  if (!version) return; const saved = await api('history', { root: d.root, path: d.path, version }); checkpoint(); d.text = saved.text; if (d.path.endsWith('.json')) d.value = parse(saved.text); changed(true);
}
async function boot() {
  bootstrap = await api('bootstrap'); token = bootstrap.token;
  try { state.settings = JSON.parse(localStorage.getItem('quest-workbench-settings') || '{}'); } catch { state.settings = {}; }
  catalog = await api('catalog'); bootstrap.roots.forEach(r => $('#workspace').append(new Option(r.name, r.id)));
  authoring = createAuthoringUI({ state, bootstrap, catalog, $, el, button, api, parse, pretty, clone, at, human, nameOf, kind, ask, inputField, set, checkpoint, changed, flush, openFile, renderDocument, renderFiles, renderFields, renderFlow, renderWorld, renderSceneGraph, renderTimeline, renderDialogue, renderCollection, importPosition, addPlacement, schemaFor, defaultValue, newFile, startJob, tool, validate, showHistory, reload, compareDisk, undo, save, error, toast, expandBottom, renderBottom, moveItem, remove });
  $('#workspace').onchange = async e => { try { await flush(); state.root = Number(e.target.value); state.doc = null; state.selection = []; await authoring.loadProject(); renderFiles(); renderDocument(); } catch (e) { error(e); } };
  $('#search').oninput = renderFiles; $('#refresh').onclick = () => refresh().catch(error); $('#new-file').onclick = () => newFile().catch(error);
  $('#save').onclick = () => save().catch(error); $('#reload').onclick = () => reload().catch(error); $('#compare').onclick = () => compareDisk().catch(error); $('#undo').onclick = () => undo(); $('#redo').onclick = () => undo(true); $('#validate').onclick = () => validate().catch(error);
  $('#build').onclick = () => authoring.build().catch(error);
  $('#clear-selection').onclick = () => { state.selection = []; document.body.classList.remove('inspecting'); renderInspector(); };
  $('[data-bottom="activity"]').onclick = () => { state.bottom = 'activity'; renderBottom(); }; $('[data-bottom="diagnostics"]').onclick = () => { state.bottom = 'diagnostics'; renderBottom(); }; $('[data-bottom="tools"]').onclick = () => { state.bottom = 'tools'; expandBottom(); renderBottom(); };
  $('#toggle-bottom').onclick = () => $('#bottom').classList.toggle('expanded');
  $('#close-bottom').onclick = () => { $('#bottom').hidden = true; $('#bottom').classList.remove('expanded'); document.body.classList.remove('has-bottom'); };
  document.addEventListener('keydown', e => { if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 's') { e.preventDefault(); save().catch(error); } if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'z' && !['INPUT', 'TEXTAREA'].includes(document.activeElement.tagName)) { e.preventDefault(); undo(e.shiftKey); } if (e.key === 'Escape') document.body.classList.remove('inspecting'); });
  window.addEventListener('beforeunload', e => { if (state.doc?.dirty) { e.preventDefault(); e.returnValue = ''; } });
  await refresh(); await authoring.loadProject(); renderDocument(); renderBottom();
  try { const last = JSON.parse(localStorage.getItem('quest-workbench-last')); if (last) { const root = bootstrap.roots.find(r => r.path === last.root); if (root && state.files.find(r => r.id === root.id)?.files.some(f => f.path === last.path)) await openFile(last.path, root.id); } } catch (e) { error(e); }
  if (!state.doc && authoring.quests().length) await openFile(authoring.quests()[0].path);
  setInterval(() => pollJobs().catch(console.error), 1800);
}
boot().catch(error);
