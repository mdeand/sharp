-- smoothbot_intermediate.lua
-- A difficult single-target tracking scenario.
-- The bot orbits the player at a fixed distance, strafes vertically,
-- and periodically performs sharp jumps with gravity-based falls.
-- Speed varies over time to force constant re-adjustment.

local scenario = {}
scenario.name = "(Shitty) Smoothbot Intermediate"

-- Tuning knobs ---------------------------------------------------------------
local ORBIT_DIST    = 65.0    -- radius of the orbit around the player
local BOT_RADIUS    = 3.05    -- slightly smaller = harder
local ORBIT_SPEED   = 1.3     -- base angular speed (rad/s) around player
local ORBIT_ACCEL   = 0.4     -- amplitude of speed oscillation
local ORBIT_WOBBLE  = 0.35    -- how much the orbit radius pulses in/out

-- Vertical strafing
local STRAFE_AMP    = 3.5     -- vertical oscillation amplitude
local STRAFE_FREQ   = 2.1     -- vertical oscillation speed

-- Jump / gravity system
local JUMP_INTERVAL_MIN = 1.5 -- min seconds between jumps
local JUMP_INTERVAL_MAX = 3.5 -- max seconds between jumps
local JUMP_VELOCITY     = 14.0 -- initial upward velocity on jump
local GRAVITY           = 12.0 -- slower gravity = longer hang time

-- Direction reversals
local REVERSE_INTERVAL_MIN = 3.0
local REVERSE_INTERVAL_MAX = 6.0

-- Time-warp (variable speed feel)
local WARP_BASE  = 1.0
local WARP_AMP   = 0.5
local WARP_FREQ  = 0.6

-- Internal state -------------------------------------------------------------
local eye       = { x = 0, y = 0, z = 0 }
local angle     = 0.0            -- current orbit angle (radians)
local direction = 1.0            -- +1 or -1 for orbit direction
local warped_t  = 0.0            -- accumulated warped time
local base_y    = 0.0            -- the "ground" Y for the bot

-- Jump state
local jumping       = false
local jump_vy       = 0.0        -- current vertical velocity from jump
local jump_y_offset = 0.0        -- extra Y from jump arc
local next_jump     = 0.0        -- timer until next jump
local next_reverse  = 0.0        -- timer until next direction change

-- Simple seeded pseudo-random (good enough for variety, deterministic per run)
local seed = 12345
local function rand01()
  seed = (seed * 1103515245 + 12345) % 2147483648
  return seed / 2147483648
end

local function rand_range(lo, hi)
  return lo + rand01() * (hi - lo)
end

local function schedule_jump()
  next_jump = rand_range(JUMP_INTERVAL_MIN, JUMP_INTERVAL_MAX)
end

local function schedule_reverse()
  next_reverse = rand_range(REVERSE_INTERVAL_MIN, REVERSE_INTERVAL_MAX)
end

--- Compute the bot's position given current state ---------------------------
local function bot_position(t)
  -- Pulsing orbit radius
  local r = ORBIT_DIST + math.sin(t * 0.7) * ORBIT_WOBBLE * ORBIT_DIST

  local px = eye.x + math.cos(angle) * r
  local pz = eye.z + math.sin(angle) * r

  -- Vertical: base strafe + jump offset
  local strafe_y = math.sin(STRAFE_FREQ * t) * STRAFE_AMP
  local py = base_y + strafe_y + jump_y_offset

  return { x = px, y = py, z = pz }
end

-------------------------------------------------------------------------------
-- init(ctx) — called once on scenario load
-------------------------------------------------------------------------------
function scenario.init(ctx)
  eye.x = ctx.eye_x
  eye.y = ctx.eye_y
  eye.z = ctx.eye_z
  base_y = ctx.eye_y + 1.0  -- roughly head-height

  -- Start the bot directly in front of the player
  angle = math.atan2(ctx.forward_z, ctx.forward_x)
  direction = 1.0
  warped_t = 0.0
  jumping = false
  jump_vy = 0.0
  jump_y_offset = 0.0
  seed = 12345

  schedule_jump()
  schedule_reverse()

  local pos = bot_position(0)
  return {
    { x = pos.x, y = pos.y, z = pos.z, radius = BOT_RADIUS },
  }
end

-------------------------------------------------------------------------------
-- update(targets, ctx) — called every frame
-------------------------------------------------------------------------------
function scenario.update(targets, ctx)
  local dt    = ctx.dt
  local timer = ctx.timer

  -- Time warp for variable-speed feel
  local warp = WARP_BASE + WARP_AMP * math.sin(WARP_FREQ * timer)
  if warp < 0.15 then warp = 0.15 end
  warped_t = warped_t + dt * warp

  -- Variable orbit speed (changes over time)
  local speed = ORBIT_SPEED + ORBIT_ACCEL * math.sin(0.4 * warped_t)

  -- Advance orbit angle
  angle = angle + direction * speed * dt * warp

  -- Direction reversal timer
  next_reverse = next_reverse - dt
  if next_reverse <= 0 then
    direction = -direction
    schedule_reverse()
  end

  -- Jump timer / physics
  next_jump = next_jump - dt
  if not jumping and next_jump <= 0 then
    jumping = true
    jump_vy = JUMP_VELOCITY * (0.8 + rand01() * 0.4) -- slight variation
    jump_y_offset = 0.0
  end

  if jumping then
    jump_vy = jump_vy - GRAVITY * dt
    jump_y_offset = jump_y_offset + jump_vy * dt

    -- Landed (returned to or below ground plane)
    if jump_y_offset <= 0 then
      jump_y_offset = 0.0
      jump_vy = 0.0
      jumping = false
      schedule_jump()
    end
  end

  -- Compute final position
  local pos = bot_position(warped_t)
  targets[1].x = pos.x
  targets[1].y = pos.y
  targets[1].z = pos.z

  return targets
end

return scenario
