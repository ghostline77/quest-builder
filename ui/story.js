import { families, stages, labels } from './stage-fields.js';

/** Everyday authoring over the same documents and commands as the advanced editor. */
export function createAuthoringUI(core) {
  const { state, $, el, button, api, parse, pretty, human, kind, ask, inputField,
    set, checkpoint, changed, flush, openFile, renderDocument, renderFields,
    schemaFor, defaultValue, toast, error } = core;
  const indexes = new Map();
  const defaultSettings = { ...state.settings };
  let section = 'story', activeQuest = null, openedStage = null, outlineView = 'outline';
  let sceneView = 'dialogue', loading = false, sceneSource = null, scriptFocus = null;
  const libraryQueries = { scenes: '', locations: '', cast: '' };
  const sectionStates = new Map();
  const advancedOpen = new Set();
  const changedMeetingLocations = new Set();
  const meetingBindings = new Map();

  function icon(name) {
    const image = el('img', 'ui-icon');
    image.src = `/assets/icon-${name}.svg`;
    image.alt = '';
    image.setAttribute('aria-hidden', 'true');
    return image;
  }

  function action(text, fn, className = '') {
    return button(text, fn, className);
  }

  function index() { return indexes.get(state.root) || { documents: [], quests: [], errors: [] }; }
  function value(record) {
    return state.doc?.root === state.root && state.doc.path === record.path ? state.doc.value : state.docs.get(`${state.root}:${record.path}`)?.value || record.value;
  }
  function quests() { return index().quests; }
  function quest() {
    if (state.doc && kind(state.doc.path, state.doc.value) === 'quest') return state.doc;
    if (activeQuest?.root !== state.root) return null;
    const record = quests().find(q => q.path === activeQuest.path);
    return record ? { root: state.root, path: record.path, value: value(record) } : null;
  }
  function folder() { return quest()?.path.split('/').slice(0, -1).join('/') || ''; }
  function scope(record) {
    const q = quest();
    if (!q) return true;
    if (record.path.startsWith(folder() + '/')) return true;
    const resources = q.value.resources || {};
    return [...resources.scenes || [], ...resources.worlds || []].some(row => {
      const path = typeof row === 'string' ? row : row.spec;
      return typeof path === 'string' && relative(q.path, path) === record.path;
    });
  }
  function documents(type, scoped = true) {
    return index().documents.filter(r => r.type === type && (!scoped || scope(r)));
  }
  function relative(base, path) {
    const parts = base.replaceAll('\\', '/').split('/').slice(0, -1);
    for (const part of path.replaceAll('\\', '/').split('/')) {
      if (part === '..') parts.pop(); else if (part && part !== '.') parts.push(part);
    }
    return parts.join('/');
  }
  function words(text) {
    return String(text || '').replace(/\.(scene-spec|world|character|quest|dialogue)?\.?json$/i, '')
      .replaceAll('\\', '/').split('/').at(-1).replace(/^(gq|vk|gqt)\d+[_-](?:\d+[_-])?/i, '')
      .replaceAll('_', ' ').replaceAll('-', ' ').replace(/\b\w/g, c => c.toUpperCase());
  }
  function title(record) {
    const v = value(record);
    return v.title || (record.type === 'character' ? v.name : null) || words(record.path);
  }
  async function loadProject() {
    loading = true;
    const root = state.root;
    const files = state.files.find(r => r.id === root)?.files || [];
    const candidates = files.filter(f => f.path.endsWith('.json') && f.size < 2_000_000 &&
      (/\/quest\.json$|\.quest\.json$|scene-spec\.json$|\.world\.json$|\.character\.json$|_manifest\.json$|\.dialogue\.json$/.test(f.path)));
    const results = await Promise.all(candidates.map(async file => {
      try {
        const response = await api('read', { root, path: file.path });
        const data = parse(response.text);
        return { path: file.path, value: data, type: kind(file.path, data) };
      } catch (e) { return { path: file.path, error: e.message }; }
    }));
    const docs = results.filter(r => !r.error);
    indexes.set(root, { documents: docs, quests: docs.filter(r => r.type === 'quest' && typeof r.value.id === 'string' && typeof r.value.title === 'string'), errors: results.filter(r => r.error) });
    if (root === state.root && !activeQuest) {
      const remembered = localStorage.getItem(`quest-active:${core.bootstrap.roots[root].path}`);
      const selected = quests().find(q => q.path === remembered) || quests().find(q => state.doc?.path.startsWith(q.path.split('/').slice(0, -1).join('/') + '/'));
      if (selected) activeQuest = { root, path: selected.path };
    }
    for (const record of docs.filter(r => r.type === 'quest')) rememberMeetings(root, record.path, record.value);
    loading = false;
    if (root === state.root) syncHeader();
  }

  function onOpen(doc) {
    const type = kind(doc.path, doc.value);
    if (!activeQuest || activeQuest.root !== doc.root) {
      const owner = quests().find(q => doc.path.startsWith(q.path.split('/').slice(0, -1).join('/') + '/'));
      if (owner) activeQuest = { root: doc.root, path: owner.path };
    }
    if (type === 'quest') {
      activeQuest = { root: doc.root, path: doc.path };
      localStorage.setItem(`quest-active:${core.bootstrap.roots[doc.root].path}`, doc.path);
      rememberMeetings(doc.root, doc.path, doc.value);
      section = 'story';
      if (!doc.value.stages?.some(s => s.id === openedStage)) openedStage = doc.value.stages?.[1]?.id || doc.value.stages?.[0]?.id || null;
    } else if (type === 'scene' || type === 'dialogue') {
      section = 'scenes';
      if (type === 'scene') sceneSource = doc.path;
      else sceneSource = documents('scene', false).find(r => typeof value(r).manifest === 'string' && relative(r.path, value(r).manifest) === doc.path)?.path || null;
    }
    else if (type === 'world') section = 'locations';
    else if (type === 'character') section = 'cast';
    const item = indexes.get(doc.root)?.documents.find(r => r.path === doc.path);
    if (item && doc.value) item.value = core.clone(doc.value);
    if (!indexes.has(doc.root)) loadProject().then(renderDocument).catch(error);
  }

  function syncHeader() {
    document.body.classList.toggle('advanced', Boolean(state.advanced));
    document.body.classList.toggle('file-browser', Boolean(state.advanced && state.view === 'overview'));
    $('#advanced-banner').hidden = !state.advanced;
    $('#return-to-story').textContent = `Return to ${section === 'story' ? 'story' : section}`;
    const picker = $('#quest-select');
    const current = quest()?.path;
    picker.replaceChildren(new Option(loading ? 'Loading quests…' : 'Choose a quest', ''));
    for (const record of quests()) picker.append(new Option(title(record), record.path));
    picker.value = current || '';
    for (const option of $('#workspace').options) option.textContent = human(bootstrapRoot(option.value));
    const nav = $('#project-nav'); nav.replaceChildren();
    for (const [key, text] of [['story', 'Story'], ['scenes', 'Scenes'], ['locations', 'Locations'], ['cast', 'Cast']]) {
      const b = action(text, () => navigate(key), section === key && !state.advanced ? 'active' : '');
      if (section === key && !state.advanced) b.setAttribute('aria-current', 'page');
      nav.append(b);
    }
    if (state.advanced) nav.append(el('span', 'advanced-context', 'All fields and compiler tools'));
  }
  function bootstrapRoot(id) { return (core.bootstrap.roots.find(r => r.id === Number(id))?.name || '').replaceAll('-', ' '); }

  function syncStatus() {
    const running = state.jobs.filter(j => j.status === 'running').length;
    const type = state.doc && kind(state.doc.path, state.doc.value);
    $('#build').disabled = !state.doc || Boolean(state.doc.media) || Boolean(running);
    $('#build').textContent = running ? 'Building…' : type === 'scene' ? 'Build scene' : type === 'world' ? 'Build location' : 'Build';
    $('#validate').textContent = type === 'scene' ? 'Check scene' : type === 'world' ? 'Check location' : 'Check quest';
    if (!state.advanced && type === 'dialogue') { $('#validate').textContent = 'Check dialogue'; $('#build').disabled = true; }
    if (type === 'character' || state.doc?.media) { $('#build').disabled = true; $('#validate').disabled = true; }
    if (!state.advanced && (section === 'cast' || section === 'scenes' && !['scene', 'dialogue'].includes(type) || section === 'locations' && type !== 'world')) {
      $('#build').disabled = true; $('#validate').disabled = true;
      $('#build').textContent = 'Build'; $('#validate').textContent = 'Check quest';
    }
    if (state.conflict) $('#save-status').textContent = 'File changed on disk';
    else if (!state.doc?.dirty && !state.saving && state.doc) $('#save-status').textContent = 'Saved';
    if (running) $('#save-status').textContent = `${running} job${running === 1 ? '' : 's'} running`;
  }

  async function navigate(next) {
    await flush(); state.advanced = false; section = next; state.selection = [];
    if (next === 'story' && quest() && state.doc?.path !== quest().path) await openFile(quest().path);
    else renderDocument();
  }
  function advanced(view = 'fields', selection = []) {
    state.advanced = true; state.view = view; state.selection = selection;
    $('#advanced-menu').hidden = true; $('#advanced-menu-button').setAttribute('aria-expanded', 'false');
    renderDocument(); core.renderFiles();
  }
  async function advancedQuest(view) {
    if (quest() && state.doc?.path !== quest().path) await openFile(quest().path);
    advanced(view);
  }
  async function advancedScene() {
    if (state.doc && kind(state.doc.path, state.doc.value) === 'scene') { advanced('graph'); return; }
    const scenes = documents('scene');
    if (!scenes.length) { section = 'scenes'; state.advanced = false; renderDocument(); toast('Add a scene to start editing its graph.'); return; }
    let path = scenes[0].path;
    if (scenes.length > 1) path = await ask('Choose a scene', p => { const select = el('select'); select.setAttribute('aria-label', 'Scene'); scenes.forEach(r => select.append(new Option(title(r), r.path))); p.append(select); return () => select.value; }, 'Open scene');
    if (path) { await openFile(path); advanced('graph'); }
  }
  function returnToStory() {
    state.advanced = false;
    if (section === 'story' && quest()) navigate('story').catch(error); else renderDocument();
  }

  function render() {
    document.body.classList.remove('inspecting');
    $('#properties').replaceChildren(); $('#views').replaceChildren();
    $('#editor').replaceChildren();
    $('#reload').hidden = true; $('#document-type').textContent = '';
    $('#path').textContent = ''; $('#title').textContent = quest()?.value.title || 'Your quests';
    if (state.conflict) {
      const notice = el('div', 'story-notice'); notice.append(el('span', '', 'This file changed outside the editor. Compare the versions before saving.'), action('Compare versions', core.compareDisk)); $('#editor').append(notice);
    }
    if (section === 'story') renderStory();
    else if (section === 'scenes') renderScenes();
    else if (section === 'locations') renderLocations();
    else if (section === 'cast') renderCast();
  }

  function renderQuestList() {
    $('#title').textContent = 'Your quests';
    $('#path').textContent = 'Choose a quest to continue writing.';
    const parent = $('#editor');
    for (const q of quests()) {
      const row = action('', () => openFile(q.path), 'document-list-row');
      row.append(el('strong', '', title(q)), el('span', 'muted', q.value.description || `${q.value.stages?.length || 0} stages`)); parent.append(row);
    }
    parent.append(action('Create a quest', () => createDocument('quest'), 'primary'));
    if (!quests().length) parent.prepend(el('p', 'muted', 'No quest manifests in this project yet.'));
  }
  function characterName(stage) {
    const v = quest()?.value, contact = String(stage.contact || '');
    const key = contact.match(/^@contacts\.([^.]+)/)?.[1] || contact;
    return v?.composition?.contacts?.[key]?.name || documents('character', false).find(r => value(r).id === key)?.value.name || words(key);
  }
  function stageName(stage) {
    const authored = objective(stage);
    return (typeof authored === 'string' ? authored : authored?.text) || stage.title || stages[stage.type]?.name || human(stage.type);
  }
  function stageDescription(stage) { return objective(stage)?.description ?? stage.description ?? ''; }
  function objectiveKey(stage) { return stage.objective?.match(/^@objectives\.([^.]+)\.path$/)?.[1]; }
  function objective(stage) {
    const key = objectiveKey(stage);
    return quest()?.value.composition?.objectives?.[key];
  }
  function updateStageCopy(stage, index, key, text) {
    const id = objectiveKey(stage), entry = objective(stage);
    if (id && entry !== undefined) {
      if (typeof entry === 'string') {
        checkpoint(); state.doc.value.composition.objectives[id] = { text: entry, [key]: text }; changed();
      } else set(['composition', 'objectives', id, key], text);
    } else set(['stages', index, key === 'text' ? 'title' : 'description'], text);
    document.querySelectorAll('.story-stage').forEach((row, i) => {
      const s = state.doc.value.stages[i];
      row.querySelector('.stage-heading strong').textContent = stageName(s);
      const subtitle = row.querySelector('.stage-heading small'); if (subtitle) subtitle.textContent = stageDescription(s);
      row.querySelector('.stage-grip').setAttribute('aria-label', `Move ${stageName(s)}`);
    });
    const heading = $('#editor .graph-stage-editor h2'); if (heading) heading.textContent = stageName(stage);
    $('#editor').querySelectorAll('.graph .node').forEach((node, i) => {
      const s = state.doc.value.stages[i]; if (!s) return;
      const name = stageName(s); node.setAttribute('aria-label', name); node.querySelector('text').textContent = name.length > 29 ? name.slice(0, 27) + '…' : name;
    });
  }
  async function editDisplayCopy(noun) {
    const d = state.doc.value;
    const result = await ask(`${noun} title and description`, p => {
      const title = inputField(p, `${noun} title`, d.title || documentTitle());
      const description = field(p, `${noun} description`, d.description || '', () => {}, { multiline: true });
      return () => ({ title: title.value.trim(), description: description.value });
    }, 'Save');
    if (!result) return;
    if (!result.title) throw Error(`Enter a ${noun.toLowerCase()} title.`);
    checkpoint(); Object.assign(d, result); changed(true);
  }
  function editQuestCopy() { return editDisplayCopy('Quest'); }

  function renderStory() {
    const q = quest();
    if (!q || state.doc?.path !== q.path) { renderQuestList(); return; }
    const d = state.doc.value, parent = $('#editor');
    $('#title').textContent = d.title;
    $('#path').textContent = d.description || 'Shape the story, one stage at a time.';
    const toolbar = el('div', 'story-toolbar'), modes = el('div', 'view-switch');
    for (const mode of ['outline', 'graph']) modes.append(action(human(mode), () => { outlineView = mode; renderDocument(); }, outlineView === mode ? 'active' : ''));
    const add = action('Add stage', addStage, 'primary'); add.prepend(icon('plus'));
    toolbar.append(modes, action('Edit title & description', editQuestCopy, 'quiet-link'), add); parent.append(toolbar);
    if (outlineView === 'graph') {
      const graph = el('div'); parent.append(graph); core.renderFlow(graph);
      const canvas = graph.querySelector('.graph'); if (canvas) graph.replaceChildren(canvas);
      if (state.selection[0] === 'stages' && state.selection.length >= 2) {
        const index = Number(state.selection[1]), stage = d.stages[index];
        if (stage) { const form = el('section', 'graph-stage-editor'); form.append(el('h2', '', stageName(stage))); renderStage(form, stage, index); parent.append(form); }
      }
      return;
    }
    const list = el('ol', 'story-outline'); list.setAttribute('aria-label', 'Quest stages');
    (d.stages || []).forEach((stage, i) => {
      const open = openedStage === stage.id, row = el('li', 'story-stage' + (open ? ' selected' : ''));
      const grip = action('', () => reorder(i), 'stage-grip'); grip.append(icon('grip-vertical')); grip.setAttribute('aria-label', `Move ${stageName(stage)}`);
      grip.draggable = true; grip.ondragstart = e => e.dataTransfer.setData('text/plain', String(i));
      row.ondragover = e => { e.preventDefault(); row.classList.add('drag-target'); };
      row.ondragleave = () => row.classList.remove('drag-target');
      row.ondrop = e => { e.preventDefault(); const from = Number(e.dataTransfer.getData('text/plain')); if (Number.isInteger(from) && from >= 0 && from < d.stages.length && from !== i) { checkpoint(); d.stages.splice(i, 0, d.stages.splice(from, 1)[0]); changed(true); } };
      const number = el('span', 'stage-number', i + 1);
      const heading = action('', () => { openedStage = open ? null : stage.id; renderDocument(); }, 'stage-heading');
      heading.setAttribute('aria-expanded', String(open)); heading.setAttribute('aria-controls', `stage-panel-${i}`);
      const text = el('span'); text.append(el('strong', '', stageName(stage)));
      if (!open) text.append(el('small', '', stageDescription(stage)));
      heading.append(text, icon(open ? 'chevron-up' : 'chevron-down'));
      row.append(grip, number, heading);
      if (open) { const content = el('div', 'stage-content'); content.id = `stage-panel-${i}`; renderStage(content, stage, i); row.append(content); }
      list.append(row);
    });
    parent.append(list);
    if (!d.stages?.length) parent.append(el('p', 'empty', 'Add the first stage to start your quest.'));
    const settings = el('details', 'document-advanced'); settings.append(el('summary', '', 'Advanced quest settings'));
    const links = el('div', 'advanced-links');
    links.append(action('All quest fields', () => advanced('fields')), action('Journal & objectives', () => advanced('journal')), action('Contacts & messages', () => advanced('contacts')), action('Save history', core.showHistory)); settings.append(links); parent.append(settings);
  }

  function renderStage(parent, stage, index) {
    const config = stages[stage.type], contract = core.catalog.blocks.find(b => b.type === stage.type);
    const fields = (config?.fields || []).filter(key => key in stage || contract?.fields?.[key]);
    const form = el('div', 'stage-form');
    const copy = el('div', 'stage-copy');
    field(copy, 'Stage title', stageName(stage), text => updateStageCopy(stage, index, 'text', text));
    field(copy, 'Stage description', stageDescription(stage), text => updateStageCopy(stage, index, 'description', text), { multiline: true });
    if (objective(stage) !== undefined) copy.append(el('p', 'muted', 'Edits the linked journal objective. Stages using that objective share this text.'));
    form.append(copy);
    if (stage.type === 'meet_contact') {
      const summary = el('div', 'meeting-summary'); summary.append(el('span', 'story-label', 'Summary'));
      const sentence = el('div', 'meeting-sentence'); sentence.append(el('span', '', 'Meet'));
      renderMeetingCharacter(sentence, stage, index);
      sentence.append(el('span', '', 'at'));
      renderValue(sentence, ['stages', index, 'community'], stage.community, 'Location', {}, 'community');
      summary.append(sentence); form.append(summary);
    }
    if (changedMeetingLocations.has(stage.id)) {
      for (const [key, label] of [['scene_origin', 'Scene marker'], ['setup_trigger', 'Meeting area']]) renderValue(form, ['stages', index, key], stage[key], label, {}, key);
    }
    const pin = stage.mappin?.match(/^@objectives\.([^.]+)\.mappins\.([^.]+)\.path$/);
    const pinSpec = pin && state.doc.value.composition?.objectives?.[pin[1]]?.mappins?.[pin[2]];
    if (pinSpec && !pinSpec.ref) renderValue(form, ['composition', 'objectives', pin[1], 'mappins', pin[2], 'ref'], '', 'Map marker', {}, 'scene_origin');
    for (const key of fields) {
      if (stage.type === 'meet_contact' && ['contact', 'community'].includes(key)) continue;
      const fieldSchema = contract?.fields?.[key]?.schema || {};
      const value = stage[key];
      if (value !== undefined && typeof value === 'object') {
        if (Array.isArray(value) && value.every(v => typeof v === 'string')) renderStringList(form, ['stages', index, key], value, labels[key] || human(key), key);
        else { const field = el('div', 'story-field'); field.append(el('span', 'story-label', labels[key] || human(key)), action(`${Array.isArray(value) ? value.length : Object.keys(value).length} configured · edit`, () => advanced('fields', ['stages', index, key]), 'quiet-link')); form.append(field); }
      } else renderValue(form, ['stages', index, key], value, labels[key] || human(key), fieldSchema, key);
    }
    parent.append(form);
    renderOutcomes(parent, stage, index);
    const details = el('details', 'stage-advanced'); details.open = advancedOpen.has(stage.id);
    details.append(el('summary', '', 'Advanced stage settings'));
    details.ontoggle = () => { if (details.open) advancedOpen.add(stage.id); else advancedOpen.delete(stage.id); };
    const inner = el('div', 'advanced-stage-fields');
    const actions = el('div', 'advanced-links');
    actions.append(action('Move stage', () => reorder(index)), action('Find references', () => advanced('references', ['stages', index])), action('Delete stage', async () => {
      const ok = await ask('Delete this stage?', p => { p.append(el('p', 'muted', 'References to this stage may need updating. You can undo the deletion.')); return () => true; }, 'Delete stage');
      if (ok) { openedStage = null; core.remove(['stages', index]); }
    }, 'danger'));
    inner.append(actions); renderFields(inner, ['stages', index], stage, schemaFor(['stages', index]), 0, true); details.append(inner); parent.append(details);
  }

  function bindingKey(stage) { return JSON.stringify([stage.contact, stage.community, stage.scene]); }
  function rememberMeetings(root, path, document) {
    const key = `${root}:${path}`, bindings = meetingBindings.get(key) || new Map();
    for (const stage of Array.isArray(document.stages) ? document.stages : []) if (stage.type === 'meet_contact' && stage.contact && stage.community && stage.scene) bindings.set(bindingKey(stage), core.clone(stage));
    meetingBindings.set(key, bindings);
  }
  function meetingRecipes() { const q = quest(); return q ? [...(meetingBindings.get(`${q.root}:${q.path}`)?.values() || [])] : []; }
  function applyMeeting(stage, recipe) {
    const previous = meetingRecipes().find(s => s !== stage && s.community === stage.community);
    const bindingFields = ['contact', 'community', 'scene', 'appearance', 'scene_origin', 'setup_trigger', 'rearm_trigger', 'accept_exit', 'defer_exit', 'scene_entry', 'resume_entry', 'activity', 'community_phase', 'defer_policy', 'allow_defer', 'opening_message'];
    for (const key of bindingFields) {
      if (recipe[key] !== undefined) stage[key] = core.clone(recipe[key]);
      else if (!['contact', 'community', 'scene'].includes(key)) delete stage[key];
    }
    if (recipe.phase_prefabs) stage.phase_prefabs = [...new Set([...(stage.phase_prefabs || []).filter(p => !previous?.phase_prefabs?.includes(p)), ...recipe.phase_prefabs])];
    if (recipe.required_assets) stage.required_assets = [...new Set([...(stage.required_assets || []), ...recipe.required_assets])];
  }
  function renderMeetingCharacter(parent, stage, index) {
    const field = el('div', 'story-field'), label = el('label', 'story-label', 'Character'), select = el('select'); select.setAttribute('aria-label', 'Character');
    const recipes = meetingRecipes(), options = new Map();
    for (const recipe of recipes) { const name = characterName(recipe); if (!options.has(name)) options.set(name, recipe); }
    select.append(new Option('Choose character', ''));
    for (const [name, recipe] of options) select.append(new Option(name, bindingKey(recipe)));
    const selected = [...options.values()].find(r => characterName(r) === characterName(stage)); select.value = selected ? bindingKey(selected) : '';
    select.onchange = () => { const recipe = recipes.find(r => bindingKey(r) === select.value); if (!recipe) return; checkpoint(); applyMeeting(stage, recipe); changedMeetingLocations.delete(stage.id); changed(true); };
    label.append(select); field.append(label); parent.append(field);
    if (!options.size) { field.replaceChildren(); renderValue(field, ['stages', index, 'contact'], stage.contact, 'Character', {}, 'contact'); }
  }

  function renderOutcomes(parent, stage, index) {
    const all = state.doc.value.stages;
    const routes = stage.on || { success: all[index + 1]?.id || '$end' };
    const section = el('div', 'stage-outcomes'); section.append(el('span', 'story-label', 'Outcomes'));
    const rows = el('div', 'outcome-rows');
    for (const [key, target] of Object.entries(routes)) {
      const line = el('div', 'outcome-row');
      const label = key === 'success' ? (/meet_contact|phone_job_offer/.test(stage.type) ? 'If accepted' : 'When complete') : `If ${key.replaceAll('_', ' ')}`;
      const select = el('select'); select.setAttribute('aria-label', `${stageName(stage)} ${label}`);
      select.append(new Option('End quest', '$end'));
      for (const next of all) select.append(new Option(stageName(next), next.id));
      if (typeof target === 'string' && ![...select.options].some(o => o.value === target)) select.append(new Option(words(target), target));
      if (typeof target !== 'string') { line.append(el('span', '', label), action('Edit routing in Advanced', () => advanced('fields', ['stages', index, 'on']), 'quiet-link')); rows.append(line); continue; }
      select.value = target;
      select.onchange = () => { checkpoint(); (stage.on ||= {})[key] = select.value; changed(); };
      line.append(el('label', '', label), icon('arrow-right'), select); rows.append(line);
    }
    section.append(rows); parent.append(section);
  }

  function choices(key) {
    const q = quest()?.value || {}, result = [], composition = q.composition || {};
    const add = (value, label) => { if (typeof value === 'string' && value && !result.some(o => o.value === value)) result.push({ value, label: label || words(value) }); };
    if (key === 'contact') {
      for (const [id, item] of Object.entries(composition.contacts || {})) add(`@contacts.${id}.contact`, item.name || words(id));
      for (const r of documents('character', false)) if (!composition.contacts?.[value(r).id]) add(value(r).id, value(r).name || words(value(r).id));
    } else if (key === 'scene') {
      for (const r of documents('scene')) add(value(r).archive_path, title(r));
      for (const row of q.resources?.scenes || []) if (typeof row.spec === 'object') add(row.spec.archive_path, words(row.spec.name));
    } else if (key === 'objective') {
      for (const [id, o] of Object.entries(composition.objectives || {})) add(`@objectives.${id}.path`, (typeof o === 'string' ? o : o.text) || words(id));
    } else if (/community|trigger|origin|destination|device|computer|drop_point|target/.test(key)) {
      const category = /community/.test(key) ? 'communities' : /trigger/.test(key) ? 'triggers' : /device|computer|drop_point/.test(key) ? 'devices' : 'markers';
      for (const r of documents('world')) {
        const w = value(r);
        const rows = category === 'communities' ? [...(w.community ? [w.community] : []), ...w.communities || []] : w[category] || [];
        for (const row of rows) {
          const alias = Object.entries(composition.locations || {}).find(([, v]) => v.ref === row.ref)?.[0];
          const nodeName = words(row.ref.replace(/^#/, '')).replace(/^(Com|Tr|Sm|Mp|Ap|Spot) /, '');
          add(alias ? `@locations.${alias}.ref` : row.ref, rows.length > 1 ? nodeName : title(r).replace(/ Meet$/, ' meeting'));
        }
      }
    } else if (['thread', 'message', 'messages', 'choice_group', 'accept_choice'].includes(key)) {
      function entries(items, prefix) {
        for (const [id, item] of Object.entries(items || {})) {
          const path = `${prefix}.${id}`;
          if (item.text || item.title) add(`@${path}.path`, item.text || item.title);
          if (item.type === 'choice_group') add(`@${path}.path`, words(id));
          entries(item.entries, path + '.entries');
        }
      }
      for (const [id, contact] of Object.entries(composition.contacts || {})) for (const [thread, row] of Object.entries(contact.threads || {})) {
        if (key === 'thread') add(`@contacts.${id}.threads.${thread}.path`, row.title || words(thread));
        else entries(row.entries, `contacts.${id}.threads.${thread}.entries`);
      }
    } else if (/item|grants|reward|vehicle/.test(key)) {
      const walk = item => { if (typeof item === 'string' && /^(Items|Vehicles|QuestRewards)\./.test(item)) add(item, words(item.split('.').at(-1))); else if (item && typeof item === 'object') Object.values(item).forEach(walk); };
      for (const r of quests()) walk(value(r));
    }
    return result;
  }
  function prettyValue(current, options) {
    if (options.some(o => o.value === current)) return null;
    const person = String(current).match(/^@contacts\.([^.]+)\.contact$/)?.[1];
    if (person) return quest()?.value.composition?.contacts?.[person]?.name || words(person);
    const objectiveKey = String(current).match(/^@objectives\.([^.]+)/)?.[1];
    if (objectiveKey) return quest()?.value.composition?.objectives?.[objectiveKey]?.text || words(objectiveKey);
    return words(String(current).replace(/^[@#]/, '').split('.').slice(0, -1).join('.') || current);
  }
  function renderValue(parent, path, current, label, schema = {}, key = path.at(-1)) {
    schema = core.catalog.definitions && schema.$ref ? core.catalog.definitions[schema.$ref.split('/').at(-1)] || schema : schema;
    const field = el('div', 'story-field'), caption = el('label', 'story-label', label);
    const options = choices(key); let input;
    if (options.length || schema.enum || key === 'scene' || key === 'objective' || key === 'community' || key === 'contact') {
      input = el('select'); input.append(new Option(`Choose ${label.toLowerCase()}`, ''));
      for (const option of schema.enum ? schema.enum.map(v => ({ value: v, label: human(v) })) : options) input.append(new Option(option.label, option.value));
      if (current !== undefined && current !== '' && ![...input.options].some(o => o.value === String(current))) input.append(new Option(prettyValue(current, options) || String(current), String(current)));
      input.value = current ?? ''; input.onchange = () => {
        if (key === 'community' && path[0] === 'stages' && state.doc.value.stages[Number(path[1])]?.type === 'meet_contact') {
          const stage = state.doc.value.stages[Number(path[1])], recipe = meetingRecipes().find(s => s.community === input.value);
          checkpoint();
          if (recipe) { applyMeeting(stage, recipe); changedMeetingLocations.delete(stage.id); }
          else { stage.community = input.value; delete stage.scene_origin; delete stage.setup_trigger; delete stage.rearm_trigger; changedMeetingLocations.add(stage.id); }
          changed(true);
        } else if (key === 'setup_trigger' && path[0] === 'stages') {
          checkpoint(); const stage = state.doc.value.stages[Number(path[1])]; stage.setup_trigger = input.value; if (stage.defer_policy === 'rearm') stage.rearm_trigger = input.value; changed();
        } else set(path, input.value, key === 'objective');
      };
    } else if (typeof current === 'boolean' || schema.type === 'boolean') {
      input = el('input'); input.type = 'checkbox'; input.checked = current === true; input.onchange = () => set(path, input.checked);
    } else if (typeof current === 'number' || schema.type === 'number' || schema.type === 'integer') {
      input = el('input'); input.type = 'number'; input.value = current ?? ''; input.step = schema.type === 'integer' ? '1' : 'any';
      if (schema.minimum !== undefined) input.min = schema.minimum;
      input.oninput = () => { if (input.value && Number.isFinite(Number(input.value))) set(path, Number(input.value)); };
    } else {
      input = el('input'); input.value = current ?? ''; input.placeholder = `Choose ${label.toLowerCase()}`;
      input.oninput = () => set(path, input.value);
    }
    input.setAttribute('aria-label', label); caption.append(input); field.append(caption); parent.append(field);
  }
  function renderStringList(parent, path, values, label, key) {
    const field = el('div', 'story-field list-field'); field.append(el('span', 'story-label', label));
    const list = el('div'); values.forEach((v, i) => {
      const row = el('div', 'story-list-input'); renderValue(row, [...path, i], v, `${label} ${i + 1}`, {}, key);
      row.append(action('Remove', () => core.remove([...path, i]), 'quiet-link')); list.append(row);
    });
    list.append(action(`Add ${label.toLowerCase()}`, () => { checkpoint(); values.push(''); changed(true); }, 'quiet-link')); field.append(list); parent.append(field);
  }

  async function reorder(index) {
    const position = await ask('Move stage', p => {
      const select = el('select');
      state.doc.value.stages.forEach((stage, i) => select.append(new Option(`${i + 1}. ${stageName(stage)}`, String(i))));
      select.value = String(index); select.setAttribute('aria-label', 'New stage position'); p.append(select); return () => Number(select.value);
    }, 'Move');
    if (position === null || position === index) return;
    checkpoint(); const all = state.doc.value.stages; all.splice(position, 0, all.splice(index, 1)[0]); changed(true);
  }
  async function addStage() {
    const result = await ask('Add a stage', p => {
      const family = el('select'), select = el('select'), description = el('p', 'muted');
      family.setAttribute('aria-label', 'Stage category'); select.setAttribute('aria-label', 'Stage type');
      const name = inputField(p, 'Stage name', '');
      families.forEach(f => family.append(new Option(f.name, f.name)));
      const refresh = () => {
        select.replaceChildren(); const group = families.find(f => f.name === family.value);
        for (const type of group.kinds) if (core.catalog.blocks.some(b => b.type === type)) select.append(new Option(stages[type]?.name || human(type), type));
        description.textContent = stages[select.value]?.description || '';
      };
      family.onchange = refresh; select.onchange = () => description.textContent = stages[select.value]?.description || '';
      const choices = el('div'); choices.append(el('p', 'muted', 'Choose what happens at this point in the story.'), family, select, description); p.insertBefore(choices, p.children[1] || null); refresh();
      return () => ({ type: select.value, name: name.value.trim() || stages[select.value]?.name || select.value });
    }, 'Add stage');
    if (!result) return;
    const d = state.doc.value, base = result.name.toLowerCase().replace(/[^a-z0-9]+/g, '_').replace(/^_+|_+$/g, '');
    let id = /^[a-z]/.test(base) ? base : 'stage_' + base, n = 2;
    while (d.stages?.some(s => s.id === id)) id = `${base}_${n++}`;
    const block = core.catalog.blocks.find(b => b.type === result.type);
    const stage = { id, title: result.name, type: result.type, status: 'ready', phase_resource: `mod\\${d.id}\\phases\\${id}.questphase` };
    checkpoint();
    for (const [key, field] of Object.entries(block.fields)) if (field.required && !(key in stage)) {
      if (/fact$/.test(key)) stage[key] = `${d.id}_${id}_${key.replace(/_?fact$/, '')}`;
      else stage[key] = defaultValue(field.schema, key);
    }
    if (block.fields.objective?.required) {
      d.composition ||= {}; d.composition.objectives ||= {};
      d.composition.objectives[id] = { text: result.name, description: result.name };
      stage.objective = `@objectives.${id}.path`;
      if (block.fields.description_entry?.required) stage.description_entry = `@objectives.${id}.description_entry`;
      if (block.fields.mappin?.required) { d.composition.objectives[id].mappins = { location: { ref: '', text: result.name } }; stage.mappin = `@objectives.${id}.mappins.location.path`; }
    }
    d.stages ||= []; d.stages.push(stage); openedStage = id; changed(true);
  }

  function disclosure(parent, label, path, entry) {
    const details = el('details', 'stage-advanced'); details.append(el('summary', '', label));
    // Keep large native contracts out of the initial authoring surface.
    details.addEventListener('toggle', () => {
      if (details.open && details.childElementCount === 1) {
        const fields = el('div', 'advanced-stage-fields'); renderFields(fields, path, entry, schemaFor(path), 0, true); details.append(fields);
      }
    }); parent.append(details);
  }
  function field(parent, label, current, update, options = {}) {
    const wrap = el('label', 'author-field'), input = el(options.multiline ? 'textarea' : options.choices ? 'select' : 'input');
    wrap.append(el('span', 'story-label', label)); input.setAttribute('aria-label', options.ariaLabel || label);
    if (options.choices) {
      options.choices.forEach(o => input.append(new Option(o.label, o.value)));
      if (current !== undefined && !options.choices.some(o => String(o.value) === String(current))) input.append(new Option(words(current), current));
    } else if (options.number) { input.type = 'number'; input.step = 'any'; }
    if (options.number && current === undefined) input.placeholder = '0';
    input.value = current ?? options.defaultValue ?? '';
    input[options.choices ? 'onchange' : 'oninput'] = () => {
      if (options.number && (!input.value.trim() || !Number.isFinite(Number(input.value)))) return;
      update(options.number ? Number(input.value) : input.value);
    };
    wrap.append(input); parent.append(wrap); return input;
  }
  function editField(parent, label, path, options) { return field(parent, label, core.at(path), v => set(path, v), options); }
  function quantity(n, name, plural = name + 's') { return `${n} ${n === 1 ? name : plural}`; }
  function sectionDetails(parent, name, subtitle, open = false) {
    const key = `${state.root}:${state.doc?.path}:${name}`;
    const details = el('details', 'author-section'); details.open = sectionStates.get(key) ?? open;
    details.addEventListener('toggle', () => sectionStates.set(key, details.open));
    const summary = el('summary'); summary.append(el('strong', '', name)); if (subtitle) summary.append(el('span', 'muted', subtitle));
    details.append(summary); parent.append(details); return details;
  }
  function library(parent, key, records, describe, addLabel, add) {
    const toolbar = el('div', 'library-toolbar'), search = el('input'); search.type = 'search'; search.placeholder = `Find ${key}…`;
    search.setAttribute('aria-label', `Find ${key}`); search.value = libraryQueries[key];
    const count = el('span', 'muted'), list = el('div', 'author-library');
    count.setAttribute('aria-live', 'polite');
    toolbar.append(search, count);
    if (add) { const b = action(addLabel, add, 'primary'); b.disabled = !quest(); toolbar.append(b); }
    parent.append(toolbar, list);
    if (add && !quest()) parent.append(el('p', 'muted', 'Choose a quest above before adding a new entry.'));
    const refresh = () => {
      libraryQueries[key] = search.value; list.replaceChildren();
      const filtered = records.filter(r => `${title(r)} ${value(r).description || ''} ${describe(r)}`.toLowerCase().includes(search.value.toLowerCase()));
      count.textContent = `${filtered.length} of ${records.length}`;
      for (const r of filtered) {
        const row = action('', () => openFile(r.path), 'document-list-row'); row.append(el('strong', '', title(r)));
        if (value(r).description) row.append(el('span', 'library-description', value(r).description));
        row.append(el('span', 'muted', describe(r))); list.append(row);
      }
      if (!filtered.length) list.append(el('p', 'empty', records.length ? `No matching ${key}. Try another name.` : `No ${key} yet.${add ? ` Use ${addLabel.toLowerCase()} to get started.` : ''}`));
    };
    search.oninput = refresh; refresh();
  }
  async function allRecords(next) {
    await flush(); if (state.doc) state.docs.set(`${state.doc.root}:${state.doc.path}`, state.doc);
    state.doc = null; state.selection = []; section = next; renderDocument(); $('main').scrollTop = 0;
  }
  function recordToolbar(parent, label, next) {
    const toolbar = el('div', 'story-toolbar section-toolbar'); toolbar.append(action(label, () => allRecords(next), 'quiet-link')); parent.append(toolbar); return toolbar;
  }
  function manifest(scene) {
    if (typeof scene.value.manifest === 'object') return scene.value.manifest;
    const path = typeof scene.value.manifest === 'string' && relative(scene.path, scene.value.manifest);
    const record = path && index().documents.find(r => r.path === path);
    return record ? value(record) : state.docs.get(`${state.root}:${path}`)?.value;
  }
  function nodeChoices() {
    const d = state.doc.value, nodes = [];
    for (const key of ['sections', 'choices', 'quest_nodes', 'xor_nodes', 'end_nodes', 'start_nodes']) for (const node of d[key] || []) nodes.push({ value: node.key, label: words(node.key) });
    for (const key of ['start_node', 'end_node']) if (d[key]) nodes.push({ value: d[key].key, label: words(d[key].key) });
    return nodes;
  }
  function speakerChoices() {
    const names = new Set(['V', ...documents('character', false).map(title)]);
    for (const r of documents('dialogue')) for (const line of value(r).spoken_lines || []) if (line.speaker) names.add(line.speaker);
    return [...names].sort().map(name => ({ value: name, label: name }));
  }
  function renderScenes() {
    const parent = $('#editor'), d = state.doc;
    $('#title').textContent = 'Scenes'; $('#path').textContent = 'Conversations and performances in this quest.';
    if (!d || !['scene', 'dialogue'].includes(kind(d.path, d.value))) {
      library(parent, 'scenes', documents('scene'), r => {
        const s = value(r), lines = manifest({ path: r.path, value: s })?.spoken_lines || [];
        return `${quantity(s.sections?.length || 0, 'section')} · ${quantity(lines.length, 'spoken line')} · ${(s.actors || []).map(a => words(a.name || a.key)).join(', ') || 'No cast yet'}`;
      }, 'Add scene', () => createDocument('scene'));
      const standalone = documents('dialogue').filter(r => !documents('scene').some(s => typeof value(s).manifest === 'string' && relative(s.path, value(s).manifest) === r.path));
      if (standalone.length) { const extra = sectionDetails(parent, 'Other dialogue', `${standalone.length} scripts`); for (const r of standalone) extra.append(action(title(r), () => openFile(r.path), 'document-list-row')); }
      return;
    }
    $('#title').textContent = documentTitle();
    const tabs = recordToolbar(parent, 'All scenes', 'scenes');
    if (kind(d.path, d.value) === 'dialogue') {
      $('#path').textContent = 'Write what each character says. Timing, audio and localization stay in Advanced.';
      if (sceneSource) tabs.append(action('Back to scene', () => openFile(sceneSource)));
      renderScript(parent); return;
    }
    $('#path').textContent = d.value.description || '';
    tabs.append(action('Edit title & description', () => editDisplayCopy('Scene'), 'quiet-link'));
    const switcher = el('div', 'view-switch');
    for (const [key, label] of [['dialogue', 'Conversation'], ['choices', 'Choices'], ['actors', 'Cast'], ['timeline', 'Timeline']]) {
      const b = action(label, () => { sceneView = key; renderDocument(); }, sceneView === key ? 'active' : ''); b.setAttribute('aria-pressed', String(sceneView === key)); switcher.append(b);
    }
    tabs.append(switcher); parent.append(action('Advanced scene settings', () => advanced('fields'), 'quiet-link'));
    if (sceneView === 'timeline') { core.renderTimeline(parent).then(() => { if (parent.isConnected) inlineSelection(parent); }).catch(error); return; }
    if (sceneView === 'actors') { renderSceneCast(parent); return; }
    if (sceneView === 'choices') { renderSceneChoices(parent); return; }
    if (typeof d.value.manifest === 'string') tabs.append(action('Write dialogue', () => openDialogue(), 'primary'));
    else if (d.value.manifest) { const embedded = sectionDetails(parent, 'Dialogue script', 'Embedded in this scene'); renderFields(embedded, ['manifest'], d.value.manifest, {}, 0, true); }
    const script = manifest(d), lines = script?.spoken_lines || [];
    for (const [i, scene] of (d.value.sections || []).entries()) {
      const count = scene.lines?.length || 0;
      const details = sectionDetails(parent, words(scene.key), `${count} ${count === 1 ? 'line' : 'lines'}`, i === 0);
      for (const [j, key] of (scene.lines || []).entries()) {
        const line = lines.find(l => l.key === key), row = el('div', 'dialogue-preview');
        row.append(el('strong', '', line?.speaker || 'Missing dialogue line'), el('p', '', line?.text || key));
        const controls = el('div', 'row');
        if (line && typeof d.value.manifest === 'string') controls.append(action('Edit line', () => openDialogue('spoken_lines', key), 'quiet-link'));
        if (j > 0) controls.append(action('Move up', () => { checkpoint(); [scene.lines[j - 1], scene.lines[j]] = [scene.lines[j], scene.lines[j - 1]]; changed(true); }, 'quiet-link'));
        controls.append(action('Remove from section', () => core.remove(['sections', i, 'lines', j]), 'quiet-link')); row.append(controls); details.append(row);
      }
      details.append(action('Add existing line', () => addSectionLine(i, lines), 'quiet-link'));
      disclosure(details, 'Advanced section settings', ['sections', i], scene);
    }
    parent.append(action('Add section', addSceneSection, 'quiet-link'));
    if (!d.value.sections?.length) parent.append(el('p', 'muted', 'Add a section, then write dialogue and choose the lines it plays.'));
    if (typeof d.value.manifest === 'string' && !script) parent.append(el('p', 'error', 'The linked dialogue script could not be loaded. Open Advanced to review its path.'));
  }
  async function openDialogue(collection, key) {
    const d = state.doc;
    if (typeof d.value.manifest !== 'string') { advanced('fields', ['manifest']); return; }
    scriptFocus = key ? { collection, key } : null;
    await openFile(relative(d.path, d.value.manifest));
  }
  async function addSectionLine(i, lines) {
    const key = await ask('Add a dialogue line', p => {
      const pick = field(p, 'Dialogue line', '', () => {}, { choices: [{ value: '', label: 'Choose a line' }, ...lines.map(l => ({ value: l.key, label: `${l.speaker}: ${l.text}` }))] });
      p.append(el('p', 'muted', 'Create new lines with Write dialogue. The same line can be used in more than one section.')); return () => pick.value;
    }, 'Add line');
    if (key) { checkpoint(); (state.doc.value.sections[i].lines ||= []).push(key); changed(true); }
  }
  function uniqueKey(base, rows) { let key = base, n = 2; while (rows.some(row => row.key === key)) key = `${base}_${n++}`; return key; }
  async function addSceneSection() {
    const d = state.doc.value;
    const result = await ask('Add a conversation section', p => {
      const name = inputField(p, 'Section name', '');
      const after = field(p, 'Insert after', '', () => {}, { choices: [{ value: '', label: 'Scene start' }, ...(d.sections || []).map(s => ({ value: s.key, label: words(s.key) }))] });
      return () => ({ name: name.value.trim(), after: after.value });
    }, 'Add section');
    if (!result?.name) return;
    const base = result.name.toLowerCase().replace(/[^a-z0-9]+/g, '_').replace(/^_|_$/g, '');
    if (!/^[a-z]/.test(base)) throw Error('Start the section name with a letter.');
    const key = uniqueKey(base, nodeChoices().map(n => ({ key: n.value })));
    const previous = result.after ? d.sections.find(s => s.key === result.after) : d.start_node || d.start_nodes?.[0];
    if (!previous) throw Error('This scene has no start node. Add one under Advanced → Scene graph & commands.');
    checkpoint(); const route = result.after ? 'on_end' : 'on_start';
    const next = { key, lines: [], on_end: core.clone(previous[route] || []) };
    previous[route] = [{ node: key }]; (d.sections ||= []).splice(result.after ? d.sections.findIndex(s => s.key === result.after) + 1 : 0, 0, next); changed(true);
  }
  function renderScript(parent) {
    for (const [collection, label] of [['spoken_lines', 'Spoken lines'], ['choice_lines', 'Player choice wording']]) {
      const rows = state.doc.value[collection] || [], group = el('section', 'script-group'); group.append(el('h2', '', label)); parent.append(group);
      rows.forEach((line, i) => {
        const card = el('article', 'script-line'); card.append(el('span', 'line-number', String(i + 1).padStart(2, '0')));
        const content = el('div', 'script-content');
        if (collection === 'spoken_lines') editField(content, `Speaker · line ${i + 1}`, [collection, i, 'speaker'], { choices: [{ value: '', label: 'Choose speaker' }, ...speakerChoices()] });
        const input = editField(content, `${collection === 'spoken_lines' ? 'Dialogue' : 'Choice wording'} · line ${i + 1}`, [collection, i, 'text'], { multiline: true });
        disclosure(content, 'Advanced line settings', [collection, i], line); card.append(content); group.append(card);
        if (scriptFocus?.collection === collection && scriptFocus.key === line.key) {
          scriptFocus = null; queueMicrotask(() => { input.focus(); card.scrollIntoView({ block: 'center' }); });
        }
      });
      if (!rows.length) group.append(el('p', 'muted', collection === 'spoken_lines' ? 'No spoken dialogue yet.' : 'No player choice wording yet.'));
      group.append(action(collection === 'spoken_lines' ? 'Add spoken line' : 'Add choice wording', async () => {
        const result = await ask(collection === 'spoken_lines' ? 'Add spoken line' : 'Add choice wording', p => {
          const speaker = collection === 'spoken_lines' && field(p, 'Speaker', '', () => {}, { choices: [{ value: '', label: 'Choose speaker' }, ...speakerChoices()] });
          const text = field(p, 'Text', '', () => {}, { multiline: true }); return () => ({ speaker: speaker?.value, text: text.value.trim() });
        }, 'Add line');
        if (!result?.text) return;
        if (collection === 'spoken_lines' && !result.speaker) throw Error('Choose who speaks the line.');
        checkpoint(); const all = state.doc.value[collection] ||= [];
        all.push({ key: uniqueKey(collection === 'spoken_lines' ? 'line' : 'choice', all), ...(result.speaker ? { speaker: result.speaker } : {}), text: result.text }); changed(true);
      }, 'quiet-link'));
    }
    parent.append(action('Advanced dialogue settings', () => advanced('fields'), 'quiet-link'));
  }
  function renderSceneChoices(parent) {
    const choices = state.doc.value.choices || [];
    const wording = manifest(state.doc)?.choice_lines || [];
    choices.forEach((choice, i) => {
      const details = sectionDetails(parent, words(choice.key), `${choice.options?.length || 0} options`, i === 0);
      (choice.options || []).forEach((option, j) => {
        const row = el('div', 'choice-editor'), line = wording.find(l => l.key === option.choice_key);
        row.append(el('p', 'choice-wording', line?.text || option.caption || 'Unwritten choice'));
        row.append(action('Edit wording', () => openDialogue('choice_lines', option.choice_key), 'quiet-link'));
        editField(row, 'Continue to', ['choices', i, 'options', j, 'target'], { choices: nodeChoices(), ariaLabel: `Continue after ${line?.text || option.caption || `option ${j + 1}`}` });
        disclosure(row, 'Advanced option settings', ['choices', i, 'options', j], option); details.append(row);
      });
      disclosure(details, 'Advanced choice settings', ['choices', i], choice);
    });
    if (!choices.length) parent.append(el('p', 'empty', 'No player choices yet. Add a choice node in the scene graph to branch the conversation.'));
    parent.append(action('Edit choice graph', () => advanced('graph'), 'quiet-link'));
  }
  function renderSceneCast(parent) {
    const actors = state.doc.value.actors || [];
    actors.forEach((actor, i) => {
      const details = sectionDetails(parent, words(actor.name || actor.key), actor.kind === 'player' ? 'Player' : 'Scene character', i === 0);
      editField(details, 'Display name', ['actors', i, 'name']); editField(details, 'Appearance', ['actors', i, 'appearance']);
      disclosure(details, 'Advanced actor binding', ['actors', i], actor);
    });
    if (!actors.length) parent.append(el('p', 'empty', 'No scene actors have been cast yet.'));
    parent.append(action('Add or bind actors', () => advanced('actors'), 'quiet-link'));
  }
  function inlineSelection(parent) {
    if (!state.selection.length || core.at(state.selection) === undefined) return;
    const details = el('details', 'stage-advanced'); details.open = true; details.append(el('summary', '', 'Selected entry'));
    renderFields(details, state.selection, core.at(state.selection), {}, 0, true); parent.append(details);
  }
  function renderLocations() {
    const parent = $('#editor'), d = state.doc;
    $('#title').textContent = 'Locations'; $('#path').textContent = 'Place quest objects around a captured game position.';
    if (!d || kind(d.path, d.value) !== 'world') {
      library(parent, 'locations', documents('world'), r => {
        const w = value(r); return `${quantity(w.markers?.length || 0, 'marker')} · ${quantity(w.triggers?.length || 0, 'area')} · ${quantity(w.entities?.length || 0, 'object')} · ${quantity(w.devices?.length || 0, 'device')}`;
      }, 'Add location', () => createDocument('world')); return;
    }
    $('#title').textContent = documentTitle();
    const toolbar = recordToolbar(parent, 'All locations', 'locations');
    toolbar.append(action('Edit title & description', () => editDisplayCopy('Location'), 'quiet-link'));
    toolbar.append(action('Use captured position', core.importPosition, 'primary'));
    $('#path').textContent = d.value.description || '';
    const anchor = sectionDetails(parent, 'Starting position', 'World coordinates · metres', true);
    if (d.value.origin && !Array.isArray(d.value.origin)) {
      const fields = el('div', 'coordinate-fields');
      for (const key of ['x', 'y', 'z', 'yaw']) editField(fields, key === 'yaw' ? 'Heading (°)' : key.toUpperCase(), ['origin', key], { number: true }); anchor.append(fields);
    } else disclosure(anchor, 'Edit starting position', ['origin'], d.value.origin);
    const groups = [['markers', 'Markers'], ['triggers', 'Trigger areas'], ['entities', 'Objects'], ['loot_containers', 'Loot containers'], ['devices', 'Interactive devices']];
    groups.forEach(([key, label]) => {
      const rows = d.value[key] || [], group = sectionDetails(parent, label, quantity(rows.length, 'placement'), false);
      rows.forEach((item, i) => {
        const details = sectionDetails(group, placementName(item), key === 'triggers' ? `${item.outline?.radius ?? '—'} m radius` : item.appearance || '', false);
        renderPosition(details, [key, i], item);
        if (key === 'triggers' && item.outline) {
          const fields = el('div', 'coordinate-fields');
          for (const prop of ['radius', 'width', 'depth', 'height'].filter(p => p in item.outline)) editField(fields, `${human(prop)} (m)`, [key, i, 'outline', prop], { number: true }); details.append(fields);
        }
        if (key === 'entities') { editField(details, 'Entity resource', [key, i, 'entity']); editField(details, 'Appearance', [key, i, 'appearance']); }
        if (key === 'loot_containers') {
          editField(details, 'Container resource', [key, i, 'entity_template']);
          editField(details, 'Appearance', [key, i, 'appearance']);
          (item.items || []).forEach((loot, j) => {
            const row = el('div', 'coordinate-fields');
            editField(row, 'Item record', [key, i, 'items', j, 'id']);
            editField(row, 'Quantity', [key, i, 'items', j, 'quantity'], { number: true });
            editField(row, 'Drop chance (0–1)', [key, i, 'items', j, 'chance'], { number: true });
            row.append(action('Remove item', () => { checkpoint(); item.items.splice(j, 1); changed(true); }, 'quiet-link'));
            details.append(row);
          });
          details.append(action('Add loot item', () => { checkpoint(); (item.items ||= []).push({ id: 'Items.Preset_Katana_Default', quantity: 1, chance: 1 }); changed(true); }, 'quiet-link'));
        }
        if (key === 'devices') { editField(details, 'Network name', [key, i, 'network_name']); editField(details, 'Appearance', [key, i, 'appearance']); }
        disclosure(details, 'Advanced placement settings', [key, i], item);
      });
      group.append(action(`Add ${key === 'triggers' ? 'trigger area' : key === 'entities' ? 'object' : key === 'loot_containers' ? 'loot container' : key === 'devices' ? 'device' : 'marker'}`, async () => {
        const before = d.value[key]?.length || 0;
        await core.addPlacement(({ markers: 'Marker', triggers: 'Trigger', entities: 'Entity', loot_containers: 'Loot container', devices: 'Device' })[key]);
        if ((d.value[key]?.length || 0) > before) {
          sectionStates.set(`${state.root}:${d.path}:${label}`, true);
          sectionStates.set(`${state.root}:${d.path}:${placementName(d.value[key].at(-1))}`, true); renderDocument();
        }
      }, 'quiet-link'));
    });
    const residents = [...(d.value.community ? [{ path: ['community'], value: d.value.community }] : []), ...(d.value.communities || []).map((v, i) => ({ path: ['communities', i], value: v }))];
    const people = sectionDetails(parent, 'Characters at this location', `${residents.length} groups`);
    residents.forEach(r => {
      const info = el('div', 'location-resident'); info.append(el('strong', '', placementName(r.value)));
      const entries = r.value.entries || [r.value];
      entries.forEach(entry => info.append(el('p', 'muted', `${words(entry.entry || entry.character)} · ${entry.appearance || 'default'}`)));
      disclosure(info, 'Advanced character placement', r.path, r.value); people.append(info);
    });
    if (!residents.length) people.append(el('p', 'muted', 'No characters placed here yet. Character groups and workspots are available in Advanced.'));
    const map = sectionDetails(parent, 'Placement map', 'Top-down layout, anchors and world-plan tools');
    const drawMap = () => { if (map.open && map.childElementCount === 1) { const content = el('div', 'placement-map'); map.append(content); core.renderWorld(content); inlineSelection(content); } };
    map.addEventListener('toggle', drawMap); drawMap();
    parent.append(action('Advanced world settings', () => advanced('fields'), 'quiet-link'));
  }
  function placementName(item) {
    if (item.title || item.network_name) return item.title || item.network_name;
    const text = words((item.ref || item.entry || 'Placement').replace(/^#/, ''));
    const role = text.startsWith('Sm ') ? ' · scene marker' : text.startsWith('Mp ') ? ' · map pin' : '';
    return text.replace(/^(Sm|Mp|Tr|Ap|Com|Spot) /, '') + role;
  }
  function renderPosition(parent, path, item) {
    if (!item.position) { parent.append(action('Set position', () => set([...path, 'position'], { from: 'origin' }, true), 'quiet-link')); return; }
    const p = item.position || {}, box = el('div', 'coordinate-fields');
    if (!Array.isArray(p) && !('x' in p)) {
      editField(parent, 'Relative to', [...path, 'position', 'from'], { defaultValue: 'origin', choices: [{ value: 'origin', label: 'Starting position' }, ...(state.doc.value.markers || []).filter(m => m.ref !== item.ref).map(m => ({ value: m.ref, label: placementName(m) }))] });
      for (const [key, label] of [['forward', 'Forward (m)'], ['right', 'Right (m)'], ['up', 'Up (m)']]) {
        if (!item.position) continue;
        // Polar offsets remain untouched unless the author deliberately changes representation.
        if ('distance' in p || 'bearing' in p) continue;
        editField(box, label, [...path, 'position', key], { number: true });
      }
      if ('distance' in p || 'bearing' in p) for (const [key, label] of [['distance', 'Distance (m)'], ['bearing', 'Bearing (°)'], ['up', 'Up (m)']]) editField(box, label, [...path, 'position', key], { number: true });
    } else for (const [i, key] of ['x', 'y', 'z'].entries()) editField(box, key.toUpperCase(), [...path, 'position', Array.isArray(p) ? i : key], { number: true });
    if (typeof item.yaw === 'number') editField(box, 'Heading (°)', [...path, 'yaw'], { number: true });
    parent.append(box);
  }
  function renderCast() {
    const parent = $('#editor'); $('#title').textContent = 'Cast'; $('#path').textContent = 'Characters available in this project.';
    const records = documents('character', false);
    if (!state.doc || kind(state.doc.path, state.doc.value) !== 'character') {
      library(parent, 'cast', records, r => {
        const v = value(r); return `${words(v.template)} · ${quantity(v.appearances?.length || 0, 'appearance')} · ${quantity(v.loadout?.length || 0, 'loadout item')}${castScenes(v).length ? ` · ${quantity(castScenes(v).length, 'scene')}` : ''}`;
      });
    } else {
      const d = state.doc, v = d.value; $('#title').textContent = v.name || words(v.id); $('#path').textContent = 'Character identity, appearances and role in this quest.';
      const toolbar = recordToolbar(parent, 'All cast', 'cast');
      if (state.settings.characterBuilderUrl) { const link = el('a', 'primary character-link', 'Open character-builder'); link.href = state.settings.characterBuilderUrl; link.target = '_blank'; link.rel = 'noopener'; toolbar.append(link); }
      const identity = sectionDetails(parent, 'Character', words(v.template), true); editField(identity, 'Name', ['name']);
      const scenes = castScenes(v); if (scenes.length) {
        const used = el('div', 'cast-usage'); used.append(el('span', 'story-label', 'Appears in'));
        scenes.forEach(r => used.append(action(title(r), () => openFile(r.path), 'quiet-link'))); identity.append(used);
      } else identity.append(el('p', 'muted', 'Not cast in a scene in this quest yet.'));
      const appearances = sectionDetails(parent, 'Appearance & clothing', quantity(v.appearances?.length || 0, 'saved appearance'), true);
      editField(appearances, 'Default appearance', ['active_appearance'], { choices: (v.appearances || []).map(a => ({ value: a.id, label: a.name || words(a.id) })) });
      for (const appearance of v.appearances || []) {
        const row = el('div', 'appearance-summary'); row.append(el('strong', '', appearance.name || words(appearance.id)));
        const equipment = Object.keys(appearance.equipment || {}); row.append(el('p', 'muted', equipment.length ? equipment.map(slot => human(slot.replace(/^Clo_/, ''))).join(' · ') : 'No clothing selected'));
        appearances.append(row);
      }
      appearances.append(el('p', 'muted', 'Use character-builder to change the body, face, hair and clothing.'));
      const loadout = sectionDetails(parent, 'Weapons & loadout', `${v.loadout?.length || 0} items`);
      for (const item of v.loadout || []) loadout.append(el('p', '', typeof item === 'string' ? words(item.split('.').at(-1)) : words(item.record || item.item || item.id)));
      if (!v.loadout?.length) loadout.append(el('p', 'muted', 'No weapons assigned.'));
      loadout.append(action('Edit loadout in Advanced', () => advanced('fields', ['loadout']), 'quiet-link'));
      const behavior = sectionDetails(parent, 'NPC behavior', human(v.ai?.profile || 'Default'));
      for (const [key, entry] of Object.entries(v.ai || {})) if (entry !== '' && entry !== null) { const row = el('p', 'behavior-summary'); row.append(el('strong', '', human(key)), el('span', '', String(entry))); behavior.append(row); }
      behavior.append(action('Edit behavior in Advanced', () => advanced('fields', ['ai']), 'quiet-link'));
      parent.append(action('Advanced character settings', () => advanced('fields'), 'quiet-link'));
    }
    if (!records.length) parent.append(el('p', 'muted', 'Save character-builder manifests in this project’s characters folder to make them available here.'));
    if (!state.settings.characterBuilderUrl) parent.append(action('Connect character-builder', settings, 'quiet-link'));
  }
  function castScenes(character) {
    const identifiers = [character.id, character.name, character.identity?.record, character.identity?.voice_tag].filter(Boolean).map(s => s.toLowerCase());
    return documents('scene').filter(r => (value(r).actors || []).some(a => [a.key, a.name, a.entry, a.record].some(s => typeof s === 'string' && identifiers.includes(s.toLowerCase()))));
  }
  function documentTitle() {
    const d = state.doc, type = d && kind(d.path, d.value);
    if (type === 'quest') return d.value.title || 'Your quest';
    if (type === 'dialogue') {
      const scene = sceneSource && index().documents.find(r => r.path === sceneSource);
      return sceneSource ? `${scene ? title(scene) : words(sceneSource)} · dialogue` : 'Dialogue';
    }
    if (type === 'character') return d.value.name || words(d.value.id);
    return d ? d.value?.title || words(d.path) : 'Your quests';
  }

  async function settings() {
    const result = await ask('Project settings', p => {
      p.append(el('p', 'muted', 'These local tool paths are saved for this project. They are not added to quest source.'));
      const game = inputField(p, 'Game directory', state.settings.game || '');
      const schema = inputField(p, 'Reflection override (optional)', state.settings.schema || '');
      const character = inputField(p, 'Character-builder URL', state.settings.characterBuilderUrl || '');
      return () => ({ game: game.value.trim(), schema: schema.value.trim(), characterBuilderUrl: character.value.trim() });
    }, 'Save settings');
    if (!result) return false;
    if (result.characterBuilderUrl && !/^https?:\/\//.test(result.characterBuilderUrl)) throw Error('Use an http or https character-builder URL.');
    Object.assign(state.settings, result); persistSettings(); toast('Project settings saved'); return true;
  }
  function persistSettings() {
    localStorage.setItem(`quest-project-settings:${core.bootstrap.roots[state.root].path}`, JSON.stringify(state.settings));
  }
  async function build() {
    await flush();
    if (!state.doc) return;
    const type = kind(state.doc.path, state.doc.value);
    if (!['quest', 'scene', 'world'].includes(type)) { core.tool(type === 'package' ? 'package' : 'series'); return; }
    let documentsOnly = false;
    if (!state.settings.game) {
      const choice = await ask('Set up resource generation', p => {
        p.append(el('p', 'muted', 'Choose your game directory for native generation, or generate inspection documents without it. Reflection metadata is automatic.'));
        const select = el('select'); select.setAttribute('aria-label', 'Build output'); select.append(new Option('Set up native generation', 'setup'), new Option('Generate resource documents', 'documents')); p.append(select); return () => select.value;
      }, 'Continue');
      if (!choice) return;
      if (choice === 'setup') { if (!await settings() || !state.settings.game) return; }
      else documentsOnly = true;
    }
    const output = `work/builds/${state.doc.value.id || state.doc.value.name || 'quest'}-${new Date().toISOString().replace(/[:.]/g, '-')}`;
    const args = [state.doc.path, '--out', output];
    if (!documentsOnly && state.settings.schema) args.push('--schema', state.settings.schema);
    if (!documentsOnly && state.settings.game) args.push('--game', state.settings.game);
    if (documentsOnly) args.push('--documents-only');
    await core.startJob(({ quest: 'compile', scene: 'scene', world: 'world' })[type], args); core.expandBottom();
  }

  async function createDocument(type) {
    const q = quest();
    const name = await ask(type === 'quest' ? 'Create a quest' : type === 'scene' ? 'Add a scene' : 'Add a location', p => {
      const input = inputField(p, 'Name', ''); return () => input.value.trim();
    }, 'Create');
    if (!name) return;
    await flush();
    const key = name.toLowerCase().replace(/[^a-z0-9]+/g, '_').replace(/^_+|_+$/g, '');
    if (!/^[a-z][a-z0-9_]*$/.test(key)) throw Error('Start the name with a letter.');
    const namespace = type === 'quest' ? key : q?.value.id || key;
    const base = type === 'quest' ? `quests/${key}` : folder();
    if (!base) throw Error('Choose a quest first.');
    const existing = (await api('files')).find(r => r.id === state.root)?.files || [];
    const targets = type === 'quest' ? [`${base}/quest.json`] : type === 'world' ? [`${base}/world/${key}.world.json`] : [`${base}/scenes/${key}.scene-spec.json`, `${base}/script/${key}.dialogue.json`];
    if (targets.some(path => existing.some(f => f.path.toLowerCase() === path.toLowerCase()))) throw Error('A document with that name already exists. Choose a different name.');
    let path, document, resource;
    if (type === 'quest') { path = `${base}/quest.json`; document = { schema_version: 1, id: key, title: name, description: '', stages: [], composition: {} }; }
    else if (type === 'world') {
      path = `${base}/world/${key}.world.json`; resource = `world/${key}.world.json`;
      document = { name: namespace, prefab_root: `$/mod/${namespace}/#${key}`, block_path: `mod\\${namespace}\\world\\${key}.streamingblock`, quest_sector_path: `mod\\${namespace}\\world\\${key}.streamingsector`, origin: { x: 0, y: 0, z: 0, yaw: 0 }, markers: [], triggers: [], entities: [] };
    } else {
      path = `${base}/scenes/${key}.scene-spec.json`; resource = { spec: `scenes/${key}.scene-spec.json`, dialogue: `script/${key}.dialogue.json` };
      const dialogue = { sequence_id: key, spoken_lines: [], choice_lines: [] };
      await api('save', { root: state.root, path: `${base}/script/${key}.dialogue.json`, hash: null, text: pretty(dialogue) + '\n' });
      document = { name: key, archive_path: `mod\\${namespace}\\scenes\\${key}.scene`, manifest: `../script/${key}.dialogue.json`, actors: [{ key: 'v', kind: 'player', name: 'V' }], entry_point: { name: 'start', node: 'start' }, exit_points: [{ name: 'end', node: 'end' }], start_node: { key: 'start', on_start: [{ node: 'end' }] }, end_node: { key: 'end' }, sections: [], choices: [] };
    }
    if (type === 'scene' || type === 'world') Object.assign(document, { title: name, description: '' });
    await api('save', { root: state.root, path, hash: null, text: pretty(document) + '\n' });
    if (resource && q) {
      const current = await api('read', { root: q.root, path: q.path }), data = parse(current.text);
      data.resources ||= {}; (data.resources[type === 'world' ? 'worlds' : 'scenes'] ||= []).push(resource);
      const saved = await api('save', { root: q.root, path: q.path, hash: current.hash, text: pretty(data) + '\n' });
      const cached = state.docs.get(`${q.root}:${q.path}`);
      if (cached) Object.assign(cached, { value: data, text: pretty(data) + '\n', hash: saved.hash, dirty: false });
      if (state.doc?.path === q.path) Object.assign(state.doc, { value: data, text: pretty(data) + '\n', hash: saved.hash, dirty: false });
    }
    state.files = await api('files'); await loadProject(); await openFile(path, state.root, true);
  }

  function onSelect(path) {
    if (path[0] === 'stages') openedStage = state.doc?.value.stages?.[Number(path[1])]?.id || null;
  }
  function describeIssue(issue) {
    const match = issue.path?.match(/^\/stages\/(\d+)$/);
    const source = state.doc?.path === issue.file ? state.doc.value : state.docs.get(`${issue.root}:${issue.file}`)?.value;
    const stage = match && source?.stages?.[Number(match[1])];
    if (!stage || issue.code !== 'schema') return issue;
    const block = core.catalog.blocks.find(b => b.type === stage.type);
    for (const [key, field] of Object.entries(block?.fields || {})) {
      let shape = field.schema;
      if (shape.$ref) shape = core.catalog.definitions[shape.$ref.split('/').at(-1)] || shape;
      const v = stage[key]; let message;
      if (v === undefined && field.required) message = `${labels[key] || human(key)} is required.`;
      else if (typeof v === 'number' && shape.minimum !== undefined && v < shape.minimum) message = `${labels[key] || human(key)} must be at least ${shape.minimum}.`;
      else if (typeof v === 'number' && shape.maximum !== undefined && v > shape.maximum) message = `${labels[key] || human(key)} must be at most ${shape.maximum}.`;
      else if (typeof v === 'string' && shape.minLength && v.length < shape.minLength) message = `Choose ${String(labels[key] || human(key)).toLowerCase()}.`;
      if (message) return { ...issue, native_message: issue.message, message, path: issue.path + '/' + key };
    }
    return { ...issue, native_message: issue.message, message: `${stageName(stage)} needs attention. Open its settings to review the reported issue.` };
  }

  const menu = $('#advanced-menu');
  for (const [text, fn] of [
    ['All fields', () => advanced('fields')], ['Source JSON', () => advanced('source')],
    ['File browser', () => advanced('overview')], ['Quest graph', () => advancedQuest('flow')],
    ['Journal & objectives', () => advancedQuest('journal')], ['Contacts & messages', () => advancedQuest('contacts')],
    ['Scene graph & commands', advancedScene], ['References', () => advanced('references')],
    ['Compiler tools', () => { menu.hidden = true; core.tool(state.command); }],
    ['Activity & issues', () => { menu.hidden = true; state.bottom = 'activity'; core.expandBottom(); core.renderBottom(); }],
    ['Project settings', () => { menu.hidden = true; return settings(); }],
    ['Save history', core.showHistory], ['Undo', () => core.undo()], ['Redo', () => core.undo(true)],
    ['Reload from disk', core.reload], ['New authoring file', core.newFile],
  ]) menu.append(action(text, async () => { menu.hidden = true; $('#advanced-menu-button').setAttribute('aria-expanded', 'false'); return fn(); }));
  $('#advanced-menu-button').append(icon('chevron-down'));
  $('#advanced-menu-button').onclick = () => { menu.hidden = !menu.hidden; $('#advanced-menu-button').setAttribute('aria-expanded', String(!menu.hidden)); };
  document.addEventListener('click', e => { if (!e.target.closest('.advanced-menu')) { menu.hidden = true; $('#advanced-menu-button').setAttribute('aria-expanded', 'false'); } });
  document.addEventListener('keydown', e => { if (e.key === 'Escape') { menu.hidden = true; $('#advanced-menu-button').setAttribute('aria-expanded', 'false'); } });
  $('#return-to-story').onclick = returnToStory;
  $('#quest-select').onchange = e => { if (e.target.value) { state.advanced = false; openFile(e.target.value).catch(error); } };

  return { render, onOpen, onSelect, syncHeader, syncStatus, loadProject: async () => {
    const settings = localStorage.getItem(`quest-project-settings:${core.bootstrap.roots[state.root].path}`);
    try { state.settings = settings ? parse(settings) : { ...defaultSettings }; } catch { state.settings = { ...defaultSettings }; }
    activeQuest = null; await loadProject();
  }, quests, build, stageName, documentTitle, describeIssue, revealIssue: path => advanced('fields', path) };
}
