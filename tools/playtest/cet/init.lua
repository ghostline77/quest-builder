-- Developer-only test controls. No saving, quest activation or arbitrary code execution.
local state = { boot = tostring(os.time()) .. '-' .. tostring(math.random(1000000)),
    overlay = false, ticks = 0, attachments = 0, saves = {} }
local proxy, pending, lastId, elapsed
elapsed = 0
local function write(path, value)
    local f = assert(io.open(path, 'w')); f:write(json.encode(value)); f:close()
end
local function read(path)
    local f = io.open(path, 'r'); if not f then return nil end
    local raw = f:read('*a'); f:close()
    local ok, value = pcall(json.decode, raw); if ok then return value end
end
local previous = read('command.json')
lastId = previous and previous.id
local function setFreeFly(enabled)
    local mod = assert(GetMod('freefly'), 'FreeFly is not installed')
    assert(Game.GetPlayer(), 'No loaded player')
    assert(not enabled or not Game.GetPlayer():GetMountedVehicle(), 'Exit the vehicle before enabling FreeFly')
    if mod.runtimeData.active ~= enabled then
        mod.runtimeData.active = enabled
        mod.logic.toggleFlight(mod, enabled)
    end
    state.freefly = mod.runtimeData.active
end
local function respond(id, status, detail)
    state.result = { id = id, status = status, detail = detail }
end
local function receiveSaves(saves)
    state.saves = {}
    for i, name in ipairs(saves) do state.saves[i] = { index = i - 1, name = name } end
    if not pending then return end
    local request = pending; pending = nil
    if request.action == 'list-saves' then respond(request.id, 'complete'); return end
    for _, save in ipairs(state.saves) do
        if save.name == request.save then
            local mod = GetMod('freefly')
            if mod and mod.runtimeData.active then setFreeFly(false) end
            -- Native UI save indexes are positions in this list, not folder suffixes.
            respond(request.id, 'loading', { save = save.name, index = save.index })
            write('state.json', state)
            Game.GetSystemRequestsHandler():LoadSavedGame(save.index)
            return
        end
    end
    respond(request.id, 'error', 'Save not found: ' .. tostring(request.save))
end
registerForEvent('onInit', function()
    local ok, err = pcall(function()
        proxy = NewProxy({ OnSavesForLoadReady = { args = { 'array:String' }, callback = function(saves)
            local success, failure = pcall(receiveSaves, saves)
            if not success then respond(lastId, 'error', tostring(failure)) end
        end } })
        Game.GetSystemRequestsHandler():RegisterToCallback('OnSavesForLoadReady', proxy:Target(), proxy:Function('OnSavesForLoadReady'))
        Observe('PlayerPuppet', 'OnGameAttached', function()
            state.attachments = state.attachments + 1
            if state.result and state.result.status == 'loading' then state.attachedAt = state.ticks end
        end)
        Observe('PlayerPuppet', 'OnDetach', function() state.detached = true end)
    end)
    if not ok then state.error = tostring(err) end
    write('state.json', state)
end)
registerForEvent('onOverlayOpen', function() state.overlay = true end)
registerForEvent('onOverlayClose', function() state.overlay = false end)
registerForEvent('onDraw', function()
    if not state.overlay then return end
    -- Append to CET's existing toolbar window to obtain its live position and size.
    ImGui.Begin('Cyber Engine Tweaks')
    local x, y = ImGui.GetWindowPos()
    local w, h = ImGui.GetWindowSize()
    state.toolbar = { x = x, y = y, width = w, height = h, frame = ImGui.GetFrameHeight() }
    ImGui.End()
end)
registerForEvent('onUpdate', function(dt)
    elapsed = elapsed + dt
    if elapsed < 0.25 then return end
    elapsed = 0; state.ticks = state.ticks + 1
    local requestId
    local ok, err = pcall(function()
        local player = Game.GetPlayer()
        state.player = player ~= nil
        state.paused = Game.GetSystemRequestsHandler():IsGamePaused()
        state.pregame = Game.GetSystemRequestsHandler():IsPreGame()
        if state.attachedAt and state.player and not state.pregame and state.ticks >= state.attachedAt + 8 then
            state.result.status = 'complete'; state.attachedAt = nil
        end
        local freefly = GetMod('freefly')
        state.freefly = freefly and freefly.runtimeData.active or false
        if player then
            local p = player:GetWorldPosition()
            state.position = { x = p.x, y = p.y, z = p.z }
        end
        local request = read('command.json')
        if request and request.id ~= lastId then
            lastId = request.id
            requestId = request.id
            if request.action == 'list-saves' or request.action == 'load-save' then
                pending = request
                respond(request.id, 'querying')
                Game.GetSystemRequestsHandler():RequestSavesForLoad()
            elseif request.action == 'freefly' then
                assert(type(request.enabled) == 'boolean', 'enabled must be a boolean')
                setFreeFly(request.enabled)
                respond(request.id, 'complete', { enabled = state.freefly })
            elseif request.action == 'quit' then
                local mod = GetMod('freefly')
                if mod and mod.runtimeData.active then setFreeFly(false) end
                respond(request.id, 'complete')
                write('state.json', state)
                Game.GetSystemRequestsHandler():ExitGame()
            else respond(request.id, 'error', 'Unsupported action') end
        end
    end)
    if not ok then
        state.error = tostring(err)
        if requestId then respond(requestId, 'error', tostring(err)); pending = nil end
    end
    write('state.json', state)
end)
registerForEvent('onShutdown', function()
    if proxy then pcall(function()
        Game.GetSystemRequestsHandler():UnregisterFromCallback('OnSavesForLoadReady', proxy:Target(), proxy:Function('OnSavesForLoadReady'))
    end) end
end)
