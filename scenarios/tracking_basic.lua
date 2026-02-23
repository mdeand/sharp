-- tracking_basic.lua
-- A single-target smooth tracking scenario (air-pure style).
-- The bot orbits an anchor point with sinusoidal motion and variable speed.

local scenario = {}
scenario.name = "Tracking - Air Pure"

-- Tuning knobs ---------------------------------------------------------------
local SPAWN_DIST  = 10.0   -- distance from camera to anchor
local BOT_RADIUS  = 0.5    -- target sphere radius
local AMP_X       = 7.0    -- horizontal sweep amplitude
local AMP_Y       = 4.0    -- vertical sweep amplitude
local AMP_Z       = 3.0    -- depth sweep amplitude
local FREQ_X      = 1.2    -- angular speed X  (rad/s)
local FREQ_Y      = 1.7    -- angular speed Y  (rad/s)
local FREQ_Z      = 0.9    -- angular speed Z  (rad/s)
local BASE_SPEED  = 1.0    -- time-warp base multiplier
local SPEED_AMP   = 0.6    -- time-warp swing amplitude
local WARP_FREQ   = 0.5    -- time-warp oscillation frequency

-- Internal state -------------------------------------------------------------
local anchor = { x = 0, y = 0, z = 0 }
local phase  = { 0.0, 1.3, 2.7 }
local warped_time = 0.0

-- Compute the bot's world position at warped time t.
local function bot_position(t)
  return {
    x = anchor.x + math.sin(FREQ_X * t + phase[1]) * AMP_X,
    y = anchor.y + math.sin(FREQ_Y * t + phase[2]) * AMP_Y,
    z = anchor.z + math.sin(FREQ_Z * t + phase[3]) * AMP_Z,
  }
end

-------------------------------------------------------------------------------
-- Called once when the scenario is loaded.
-- ctx fields: eye_x/y/z, forward_x/y/z, fovy, aspect
-- Must return a list of targets: { {x, y, z, radius}, ... }
-------------------------------------------------------------------------------
function scenario.init(ctx)
  local fx, fy, fz = ctx.forward_x, ctx.forward_y, ctx.forward_z

  anchor.x = ctx.eye_x + fx * SPAWN_DIST
  anchor.y = ctx.eye_y + fy * SPAWN_DIST
  anchor.z = ctx.eye_z + fz * SPAWN_DIST

  warped_time = 0.0
  local pos = bot_position(warped_time)

  return {
    { x = pos.x, y = pos.y, z = pos.z, radius = BOT_RADIUS },
  }
end

-------------------------------------------------------------------------------
-- Called every frame.
-- targets: current target list (same shape returned by init)
-- ctx fields: timer, dt, eye_x/y/z, forward_x/y/z
-- Must return the (possibly mutated) targets table.
-------------------------------------------------------------------------------
function scenario.update(targets, ctx)
  local dt    = ctx.dt
  local timer = ctx.timer

  -- Advance warped time at variable speed
  local speed = BASE_SPEED + SPEED_AMP * math.sin(WARP_FREQ * timer)
  if speed < 0.05 then speed = 0.05 end
  warped_time = warped_time + dt * speed

  local pos = bot_position(warped_time)
  targets[1].x = pos.x
  targets[1].y = pos.y
  targets[1].z = pos.z

  return targets
end

return scenario
