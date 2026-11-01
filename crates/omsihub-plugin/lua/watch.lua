-- Omsi-Hub's plugin: the drive watch - what the company judges a trip by besides the timetable,
-- as Omsi-Hub's own game watched it (crates/omsi-app/src/drive_watch.rs), on openOMSI's plugin
-- API:
--
-- * red lights: the game's `red_light` event (the bus passed a light showing red); how long it
--   had been red comes from `light_ahead`, a light that turned red a moment ago is no offence;
-- * speed cameras: the game's own (its `speed_camera` event): they stand by the road in the
--   same places on every drive of the map (more on fast roads, some where the limit is low),
--   seen or unseen as the player sets it, and catch the bus faster than the limit and the
--   tolerance; switched off in the game, there are none;
-- * comfort: hard braking, hard starts, stops that end with a jerk, from the bus's speed.
--
-- An offence is said at once (a card with the fine); every event is kept with the watch's
-- clock, and a trip takes those of its own stretch (`M.between`).

local career = require("career")

local M = {}

local RED_GRACE = 0.5      -- s red before it counts
local CREEP = 4            -- km/h: slower over the line is creeping
local HARD_BRAKE, HARD_START, ROUGH_STOP = 2.8, 2.0, 2.0  -- m/s²
local ROUGH_AFTER_BRAKE = 4.0  -- s: a rough stop that soon after a hard brake is that brake's
local REARM = 1.2
local KEPT = 4000

local clock = 0
local events = {}
local speed, accel, braking, starting = 0, 0, false, false
local slow = {}
local red_since = nil      -- the clock when the light ahead turned red

-- the place this language writes its texts with (`tr`), and the card for offences (`notify`)
local tr, notify = function(t) return t end, function() end

function M.setup(translate, say)
  tr, notify = translate, say
end

function M.clock()
  return clock
end

local function keep(kind, offence)
  events[#events + 1] = { at = clock, kind = kind, offence = offence }
  if #events > KEPT then table.remove(events, 1) end
end

local function euros(cents)
  return string.format("€%d", career.round(cents / 100))
end

local function offence(o)
  keep("offence", o)
  if o.kind == "red_light" then
    notify("traffic", tr("Red light"), tr("You crossed the stop line on red · fine %{amount}", { amount = euros(o.fine) }))
  else
    -- (in miles an hour for a player who reads them - Britain's and America's units -, the
    -- limit as the signs show it: in steps of 5 mph)
    local mph = M.units == "uk" or M.units == "us" or M.units == "imperial"
    local k = mph and 0.621371 or 1
    local limit = mph and math.max(career.round(o.limit * k / 5) * 5, 5) or o.limit
    local text = mph and "%{speed} mph where %{limit} is allowed · fine %{amount}" or "%{speed} km/h where %{limit} is allowed · fine %{amount}"
    notify("photo_camera", tr("Speed camera"), tr(text, { speed = string.format("%.0f", o.speed * k), limit = string.format("%.0f", limit), amount = euros(o.fine) }))
  end
end

-- comfort, from the speed (m/s) frame by frame
local function comfort(dt, v)
  dt = math.max(dt, 1e-3)
  local raw = (v - speed) / dt
  local k = math.min(4 * dt, 0.5)
  accel = accel + (raw - accel) * k
  local a, was = accel, speed
  speed = v
  if a < -HARD_BRAKE and v > 2 and not braking then
    braking = true
    keep("hard_brake")
  elseif a > -REARM then
    braking = false
  end
  if a > HARD_START and v > 0.5 and not starting then
    starting = true
    keep("hard_start")
  elseif a < HARD_START * 0.6 then
    starting = false
  end
  if v < 2.5 and v > 0.05 then
    slow[#slow + 1] = { clock, a }
  elseif v >= 2.5 then
    slow = {}
  end
  while slow[1] and clock - slow[1][1] > 0.5 do table.remove(slow, 1) end
  if was > 0.05 and v <= 0.05 then
    if #slow >= 3 then
      local sum = 0
      for _, x in ipairs(slow) do sum = sum + x[2] end
      if sum / #slow < -ROUGH_STOP then keep("rough_stop") end
    end
    slow = {}
  end
end

-- the game's speed camera caught the bus at `kmh` where `limit` is allowed (km/h)
function M.speed_camera(kmh, limit)
  local over = career.over_limit(kmh, limit)
  if not over then return end
  offence({ kind = "speeding", speed = kmh, limit = limit, red_for = 0, fine = career.speeding_fine(over, limit <= 50) })
end

-- every frame of the game with the player in a bus
function M.frame(dt)
  clock = clock + dt
  local kmh = omsi.speed and omsi.speed() or 0
  comfort(dt, kmh / 3.6)
end

function M.light_ahead(aspect)
  if aspect == "red" or aspect == "red_yellow" then
    red_since = red_since or clock
  else
    red_since = nil
  end
end

function M.red_light(kmh)
  local red_for = red_since and (clock - red_since) or RED_GRACE
  if red_for < RED_GRACE or (kmh or 0) < CREEP then return end
  offence({ kind = "red_light", speed = kmh, limit = 0, red_for = red_for, fine = career.red_light_fine(red_for) })
  red_since = nil
end

-- what the watch saw since the clock was `from`: the counts, the fines and the offences
-- (their times from `from`), as the driver's record keeps them
function M.between(from)
  local t = { watched = true, red_lights = 0, speeding = 0, fines = 0, hard_brakes = 0, hard_starts = 0, rough_stops = 0, offences = {} }
  local braked = nil
  for _, e in ipairs(events) do
    if e.at >= from then
      if e.kind == "hard_brake" then
        t.hard_brakes = t.hard_brakes + 1
        braked = e.at
      elseif e.kind == "hard_start" then
        t.hard_starts = t.hard_starts + 1
      elseif e.kind == "rough_stop" then
        if not (braked and e.at - braked <= ROUGH_AFTER_BRAKE) then t.rough_stops = t.rough_stops + 1 end
      elseif e.kind == "offence" then
        local o = e.offence
        if o.kind == "red_light" then t.red_lights = t.red_lights + 1 else t.speeding = t.speeding + 1 end
        t.fines = t.fines + o.fine
        t.offences[#t.offences + 1] = { kind = o.kind, at = math.max(e.at - from, 0), speed = o.speed, limit = o.limit, red_for = o.red_for, fine = o.fine }
      end
    end
  end
  return t
end

return M
