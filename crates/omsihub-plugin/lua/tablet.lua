-- (NexiconDK, 16 November 2026) The VCP tablet of UrbanBacon / Linje 5C (a bus's
-- Script\IBIS_Tablet\IBIS_Tablet.osc) with Bus Company Online, both ways - without a change to
-- the tablet's own script (made by its author's generator):
--
--   * its message page (page 9) shows the control room's two newest messages
--     (Tablet_M1_* and Tablet_M2_*: sender, time, title, text), and a new one comes up on its
--     screen (Tablet_Toast); a tap on one marks it read, and an instruction tapped twice within
--     TAP_AGAIN seconds is accepted (declined with F9 or on the phone, as before);
--   * its call page's buttons (page 8: Tab_anmod, Tab_op1..7 - which only said "Sent: ..." on
--     the tablet) go to control as the driver's quick messages (`QUICK`);
--   * the company's lines on the map (the assignment's `lines`) go to the tablet's own plugin
--     (IBIS_Tablet), which lists only those: topic `omsihub_company_lines`, `{ lines = {...} }`
--     (empty: a drive of the player's own - every line).
--
-- The game tells the plugins the bus's triggers run from a click (`bus_trigger`).

local M = {}

-- the call page's buttons and the quick message each is to control (`Quick::key`)
local QUICK = {
  Tab_anmod = "request_talk",
  Tab_op1 = "delay_traffic",     -- "Tur, Forsinkelse"
  Tab_op2 = "question_trip",     -- "Tur, Spørgsmål"
  Tab_op3 = "road_closed",       -- "Tur, Omlægning"
  Tab_op4 = "question_bus",      -- "Bus, Spørgsmål"
  Tab_op5 = "breakdown",         -- "Bus, Nedbrud"
  Tab_op6 = "passengers_left",   -- "Passagerer efterladt"
  Tab_op7 = "accident",          -- "Bus, Skade"
}
-- the message page's two messages: the trigger of each, newest first
local SLOTS = { "Tab_msg1", "Tab_msg2" }
-- seconds within which a second tap on an instruction accepts it
local TAP_AGAIN = 8
-- how often the company's lines go to the tablet's plugin again (it may load or reload later)
local LINES_EVERY = 10

local tr = function(t) return t end
local hook = {}
local msgs = nil
local lines = {}
local T = 0
local lines_at = -1e9
local on_tablet = {}     -- slot -> message id shown there
local shown_sig = nil    -- what the message page shows (redrawn when it changes)
local seen = {}          -- message ids the tablet has seen
local tapped = {}        -- message id -> when it was tapped (an instruction's first tap)

local function fn(name)
  local t = omsi
  for part in name:gmatch("[^%.]+") do
    if type(t) ~= "table" then return nil end
    t = t[part]
  end
  return type(t) == "function" and t or nil
end

-- `messages`: messages.lua; `assignment`: the company's lines
function M.setup(translate, messages, assignment)
  tr = translate
  msgs = messages
  lines = {}
  for _, l in ipairs((assignment and assignment.lines) or {}) do
    if type(l) == "string" and l ~= "" then lines[#lines + 1] = l end
  end
end

-- the player's bus has the tablet
local function here()
  return fn("var") and omsi.var("Tablet_page") ~= nil
end

local function set_str(name, text)
  if fn("set_str") then omsi.set_str(name, text or "") end
end

local function pop(text, seconds)
  set_str("Tablet_Toast", text)
  if fn("bus.set_vars") then omsi.bus.set_vars({ Tablet_toast_t = seconds or 5 }) end
end

local TITLE = { info = "Info", instruction = "Instruction", urgent = "Urgent" }

-- the message page: the two newest of control's messages (the driver's own left out)
local function show(inbox)
  local list = {}
  for _, m in ipairs((inbox and inbox.messages) or {}) do
    if type(m) == "table" and not m.mine then list[#list + 1] = m end
  end
  local sig = {}
  for k = 1, #SLOTS do
    local m = list[k]
    sig[#sig + 1] = m and (tostring(m.id) .. ":" .. tostring(m.state)) or "-"
  end
  sig = table.concat(sig, "|")
  if sig == shown_sig then return list end
  shown_sig = sig
  for k = 1, #SLOTS do
    local m = list[k]
    on_tablet[k] = m and m.id or nil
    local title = m and tr(TITLE[m.priority] or "Info") or ""
    if m and msgs and msgs.waits(m.id) then title = title .. " - " .. tr("tap twice to accept") end
    set_str("Tablet_M" .. k .. "_Afs", m and (m.from or "") or "")
    set_str("Tablet_M" .. k .. "_Tid", m and (m.time or "") or "")
    set_str("Tablet_M" .. k .. "_Titel", title)
    set_str("Tablet_M" .. k .. "_Tekst", m and (m.text or "") or "")
  end
  if fn("bus.set_vars") then omsi.bus.set_vars({ Tablet_dirty = 1 }) end
  return list
end

-- every second: the messages on the tablet, a new one popped up, the lines to its plugin
function M.tick(dt)
  T = T + (dt or 1)
  if not here() then return end
  if T - lines_at >= LINES_EVERY and fn("plugin.broadcast") then
    lines_at = T
    omsi.plugin.broadcast("omsihub_company_lines", { lines = lines })
  end
  local inbox = msgs and msgs.inbox()
  if not inbox then return end
  local list = show(inbox)
  -- (the newest just sent comes up on the tablet's screen - one delivered before, in an earlier
  -- game, does not; the panel's card and sound are messages.lua's)
  local new = nil
  for _, m in ipairs(list) do
    if not seen[m.id] then
      seen[m.id] = true
      if m.state == "sent" then new = new or m end
    end
  end
  if new then pop(tr("New message from %{from}", { from = new.from or tr("Control") }) .. ": " .. (new.text or ""), 8) end
end

-- a trigger of the bus run from a click: the tablet's buttons; whether it was the tablet's
function M.trigger(name)
  if not (msgs and msgs.inbox() and here()) then return false end
  local key = QUICK[name]
  if key then
    msgs.quick(key)
    return true
  end
  for k, trigger in ipairs(SLOTS) do
    if name == trigger then
      local id = on_tablet[k]
      if not id then return true end
      if msgs.waits(id) then
        if tapped[id] and T - tapped[id] <= TAP_AGAIN then
          tapped[id] = nil
          if msgs.accept(id) then pop(tr("Accepted"), 4) end
          shown_sig = nil
        else
          tapped[id] = T
          msgs.mark_read(id)
          -- (after the tablet's own toast of the text)
          pop(tr("Tap the message again to accept"), TAP_AGAIN)
        end
      else
        msgs.mark_read(id)
      end
      return true
    end
  end
  return false
end

-- another bus: the message page anew
function M.vehicle()
  shown_sig, on_tablet, tapped = nil, {}, {}
  lines_at = -1e9
end

return M
