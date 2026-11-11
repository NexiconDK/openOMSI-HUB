-- Omsi-Hub's plugin: the bus company's ticket inspectors in the player's bus (round U).
--
-- What the company sent with the duty (`assignment.inspection`): the share of the riders
-- without a ticket on each trip (`people.set_dodger_share`), and the inspectors who board some of
-- its trips where the company's inspectors patrol its line. Inspectors the driver asked control
-- for ("Bestil kontrol": the quick button, or Ctrl+K) come as the company's answer in the inbox
-- (an action "inspection", `messages.lua` hands it here). At the stop they get on at the game
-- puts them in the bus (`people.inspect`): the driver is told and hears it; when they get off
-- (the game's `inspection_done`) the driver sees what they found, and the company gets it as a
-- report "inspection" (checked, caught, the stops, their names).

local M = {}

local tr = function(t) return t end
local hook = {}
local spec = nil
local todo = {}
local aboard = nil
local share_now = nil
local asked_on = nil

local COLOR = "#2E7DD7"

local function fn(name)
  local t = omsi
  for part in name:gmatch("[^%.]+") do
    if type(t) ~= "table" then return nil end
    t = t[part]
  end
  if type(t) == "function" then return t end
  return nil
end

local function duty()
  return fn("duty.get") and omsi.duty.get() or nil
end

local function toast(text, title, seconds)
  if fn("ui.toast") then
    omsi.ui.toast(text, { title = title, icon = "badge", color = COLOR, seconds = seconds or 10 })
  else
    omsi.message(title .. ": " .. text, seconds or 10)
  end
end

local function sound()
  if fn("audio.play") then omsi.audio.play("msg_instruction.wav", { volume = 0.8 }) end
end

-- the trip under way's departure (seconds of the day)
local function departure(d)
  local trips = fn("duty.trips") and omsi.duty.trips() or {}
  local t = d and trips[d.trip] or nil
  return (t and t.departure) or (d and d.departure) or nil
end

local function same_trip(a, b)
  return a == nil or b == nil or math.abs(a - b) < 90
end

function M.setup(translate, hooks, assignment)
  tr = translate
  hook = hooks or {}
  spec = assignment and type(assignment.inspection) == "table" and assignment.inspection or nil
  todo = {}
  for _, b in ipairs(spec and spec.boardings or {}) do
    todo[#todo + 1] = { departure = b.departure, board = b.board or 1, stops = b.stops or 4, n = b.n or 2, variant = b.variant or spec.variant or "", names = b.names or {}, ordered = false }
  end
end

-- whether the company's inspection is on for this drive
function M.on()
  return spec ~= nil
end

-- every few seconds: the share of riders without a ticket of the trip under way
function M.tick()
  if not (spec and fn("people.set_dodger_share")) then return end
  local d = duty()
  local dep = departure(d)
  local share = spec.share or 0
  for _, s in ipairs(spec.shares or {}) do
    if dep and math.abs((s.departure or -1e9) - dep) < 90 then share = s.share or share end
  end
  if share ~= share_now then
    share_now = share
    omsi.people.set_dodger_share(share)
  end
end

-- the stop `number` of the trip under way's name
local function stop_name(number)
  local stops = fn("duty.stops") and omsi.duty.stops() or {}
  local s = stops[number or 0]
  if type(s) == "table" and s.name then return s.name end
  local d = duty()
  return d and d.next_stop and d.next_stop.name or ""
end

-- the bus at stop `number` of the trip under way (main.lua's `stop_arrive`): the inspectors due
-- here get on
function M.arrive(number)
  if aboard or not fn("people.inspect") then return end
  local d = duty()
  if not d then return end
  local dep = departure(d)
  for i, b in ipairs(todo) do
    if same_trip(b.departure, dep) and (number or 0) >= (b.board or 1) then
      local name = stop_name(number)
      local id = omsi.people.inspect(b.stops, b.n, b.variant, name)
      if id then
        table.remove(todo, i)
        aboard = { id = id, names = b.names, ordered = b.ordered, trip = d.trip, departure = dep }
        toast(tr("Ticket inspection gets on at %{stop}", { stop = name }), tr("Ticket inspection"), 10)
        sound()
      end
      return
    end
  end
  -- (a trip gone by: what was planned for it is no more)
  for i = #todo, 1, -1 do
    local b = todo[i]
    if b.departure and dep and b.departure < dep - 90 then table.remove(todo, i) end
  end
end

-- the inspectors got off (the game's `inspection_done`): what they found, to the driver and the
-- company
function M.done(checked, caught, fled, stops, board_stop, leave_stop, team)
  local a = aboard or {}
  aboard = nil
  checked, caught = math.tointeger(checked) or 0, math.tointeger(caught) or 0
  toast(tr("Inspection: %{checked} checked, %{caught} without a ticket", { checked = checked, caught = caught }), tr("Ticket inspection"), 10)
  local d = duty()
  if hook.report then
    hook.report("inspection", {
      line = d and d.line or "", tour = d and d.tour or "", trip = a.trip or (d and d.trip) or 0,
      departure = a.departure, number = d and d.line or "",
      inspection = { checked = checked, caught = caught, fled = math.tointeger(fled) or 0, stops = math.tointeger(stops) or 0,
        board_stop = board_stop or "", leave_stop = leave_stop or "", names = a.names or {}, ordered = a.ordered == true },
    })
  end
end

-- inspectors the driver asked for, as the company answered (an inbox action "inspection")
function M.ordered(action)
  if type(action) ~= "table" then return end
  todo[#todo + 1] = { departure = action.departure, board = action.board or 1, stops = action.stops or 4, n = action.n or 2,
    variant = action.variant or (spec and spec.variant) or "", names = action.names or {}, ordered = true }
end

-- Ctrl+K, or the quick button: ask control for inspectors (once a trip)
function M.ask()
  local d = duty()
  if not d then return false end
  -- (a company session's player has no inspection of the duty: control's inbox decides)
  if spec then
    if not spec.can_order then return false end
  elseif not (hook.inbox and hook.inbox()) then
    return false
  end
  if asked_on == d.trip or aboard then
    toast(tr("Control has been asked on this trip already"), tr("Ticket inspection"), 6)
    return true
  end
  asked_on = d.trip
  if hook.quick then hook.quick("request_inspection") end
  toast(tr("Control is asked for ticket inspectors"), tr("Ticket inspection"), 6)
  return true
end

return M
