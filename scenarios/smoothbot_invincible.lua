-- smoothbot_invincible.lua
-- Replicates "SmoothBot Invincible Goated 75%" from KovaaK's.
--
-- The bot is ANCHORED to the camera and orbits around it.  Movement is
-- modelled in polar coordinates (angle + distance from camera) so the bot
-- always stays at roughly the right range.  Left/right strafing drives
-- angular velocity (orbiting), forward/back wiggles the distance, and
-- jumps + air-jumps add vertical variety.  Everything at 75 % timescale.

local scenario = {}
scenario.name = "SmoothBot Invincible 75%"

-- === Timescale ==============================================================
local TIMESCALE = 0.75

-- === Bot shape (pill / capsule) =============================================
local BOT_RADIUS = 0.6
local BOT_HEIGHT = 3.0

-- === Orbit parameters =======================================================
local PREFERRED_DIST  = 12.0   -- ideal distance from camera
local DIST_WOBBLE     = 3.0    -- max distance deviation from preferred

-- Angular (orbit) speed in rad/s – at 12 m this is ~12 m/s tangential
local ORBIT_SPEED     = 1.0
local ORBIT_ACCEL     = 4.0    -- how fast angular speed ramps up
local ORBIT_FRICTION  = 3.0    -- angular friction (damps when input flips)

-- Radial (in/out) speed
local RADIAL_SPEED    = 3.0    -- max radial drift speed
local RADIAL_ACCEL    = 8.0    -- radial acceleration
local RADIAL_SPRING   = 6.0    -- spring constant pulling back to PREFERRED_DIST

-- === Strafe timing ==========================================================
local LR_MIN_TIME = 1.0
local LR_MAX_TIME = 1.5
local FB_MIN_TIME = 0.4
local FB_MAX_TIME = 1.5

-- === Jumping ================================================================
local JUMP_FREQUENCY  = 0.33
local JUMP_VELOCITY   = 6.0
local AIR_JUMP_COUNT  = 2
local AIR_JUMP_VEL    = 8.0
local GRAVITY         = 12.0
local GROUND_Y        = 0.0
local MIN_JUMP_TIME   = 0.05
local MAX_JUMP_TIME   = 0.5

-- === Internal state =========================================================
-- Polar coords relative to camera (xz plane)
local angle    = 0.0      -- current orbit angle (radians)
local ang_vel  = 0.0      -- angular velocity (rad/s)
local dist     = 0.0      -- current distance from camera
local dist_vel = 0.0      -- radial velocity (m/s)
local pos_y    = 0.0      -- world Y position
local vel_y    = 0.0      -- vertical velocity

-- Strafe inputs: +1 / -1
local input_lr = 1
local input_fb = 1

-- Timers
local lr_timer = 0
local fb_timer = 0
local jump_timer = 0
local air_jumps_left = 0
local on_ground = true

-- PRNG -----------------------------------------------------------------------
local seed = 77713
local function rand01()
  seed = (seed * 1103515245 + 12345) % 2147483648
  return seed / 2147483648
end
local function rand_range(lo, hi)
  return lo + rand01() * (hi - lo)
end

local function schedule_lr()
  lr_timer = rand_range(LR_MIN_TIME, LR_MAX_TIME)
  input_lr = -input_lr
end
local function schedule_fb()
  fb_timer = rand_range(FB_MIN_TIME, FB_MAX_TIME)
  input_fb = -input_fb
end
local function schedule_jump()
  jump_timer = rand_range(MIN_JUMP_TIME, MAX_JUMP_TIME)
end

-------------------------------------------------------------------------------
-- init(ctx)
-------------------------------------------------------------------------------
function scenario.init(ctx)
  -- Start directly in front of the camera
  angle   = math.atan2(ctx.forward_z, ctx.forward_x)
  ang_vel = 0
  dist    = PREFERRED_DIST
  dist_vel = 0
  pos_y   = ctx.eye_y
  vel_y   = 0
  GROUND_Y = ctx.eye_y - 1.0

  on_ground = true
  air_jumps_left = AIR_JUMP_COUNT
  input_lr = 1
  input_fb = 1
  seed = 77713

  lr_timer   = rand_range(LR_MIN_TIME, LR_MAX_TIME)
  fb_timer   = rand_range(FB_MIN_TIME, FB_MAX_TIME)
  jump_timer = rand_range(MIN_JUMP_TIME, MAX_JUMP_TIME)

  local px = ctx.eye_x + math.cos(angle) * dist
  local pz = ctx.eye_z + math.sin(angle) * dist

  return {
    { x = px, y = pos_y, z = pz, radius = BOT_RADIUS, height = BOT_HEIGHT },
  }
end

-------------------------------------------------------------------------------
-- update(targets, ctx)
-------------------------------------------------------------------------------
function scenario.update(targets, ctx)
  local dt = ctx.dt * TIMESCALE
  local eye_x, eye_y, eye_z = ctx.eye_x, ctx.eye_y, ctx.eye_z

  -- === Strafe timers ========================================================
  lr_timer = lr_timer - dt
  if lr_timer <= 0 then schedule_lr() end

  fb_timer = fb_timer - dt
  if fb_timer <= 0 then schedule_fb() end

  -- === Jump timer ===========================================================
  if on_ground then
    jump_timer = jump_timer - dt
    if jump_timer <= 0 then
      if rand01() < JUMP_FREQUENCY then
        vel_y = JUMP_VELOCITY
        on_ground = false
        air_jumps_left = AIR_JUMP_COUNT
      end
      schedule_jump()
    end
  else
    if air_jumps_left > 0 and vel_y < 2.0 then
      if rand01() < 0.02 then
        vel_y = AIR_JUMP_VEL
        air_jumps_left = air_jumps_left - 1
      end
    end
  end

  -- === Angular (orbit) movement =============================================
  -- input_lr drives angular acceleration; friction damps on direction change
  local target_ang_vel = input_lr * ORBIT_SPEED
  local ang_diff = target_ang_vel - ang_vel

  -- Accelerate toward target angular velocity
  local ang_accel = ORBIT_ACCEL * dt
  if math.abs(ang_diff) < ang_accel then
    ang_vel = target_ang_vel
  else
    ang_vel = ang_vel + ang_accel * (ang_diff > 0 and 1 or -1)
  end

  -- Friction: always damp slightly so direction changes feel snappy
  ang_vel = ang_vel * math.max(0, 1.0 - ORBIT_FRICTION * dt)

  angle = angle + ang_vel * dt

  -- === Radial (distance) movement ===========================================
  -- input_fb pushes in/out, spring pulls back to PREFERRED_DIST
  local spring_force = -RADIAL_SPRING * (dist - PREFERRED_DIST)
  local input_force  = input_fb * RADIAL_ACCEL

  dist_vel = dist_vel + (spring_force + input_force) * dt
  -- Clamp radial speed
  if dist_vel >  RADIAL_SPEED then dist_vel =  RADIAL_SPEED end
  if dist_vel < -RADIAL_SPEED then dist_vel = -RADIAL_SPEED end
  -- Damping
  dist_vel = dist_vel * math.max(0, 1.0 - 2.0 * dt)

  dist = dist + dist_vel * dt
  -- Hard clamp so it never gets absurdly close or far
  local min_d = PREFERRED_DIST - DIST_WOBBLE
  local max_d = PREFERRED_DIST + DIST_WOBBLE
  if dist < min_d then dist = min_d; dist_vel = math.max(dist_vel, 0) end
  if dist > max_d then dist = max_d; dist_vel = math.min(dist_vel, 0) end

  -- === Vertical ==============================================================
  if not on_ground then
    vel_y = vel_y - GRAVITY * dt
  end
  pos_y = pos_y + vel_y * dt

  if pos_y <= GROUND_Y then
    pos_y = GROUND_Y
    vel_y = 0
    on_ground = true
    air_jumps_left = AIR_JUMP_COUNT
  end

  -- === Convert polar back to world XZ (anchored to camera) ==================
  local wx = eye_x + math.cos(angle) * dist
  local wz = eye_z + math.sin(angle) * dist

  targets[1].x = wx
  targets[1].y = pos_y
  targets[1].z = wz

  return targets
end

return scenario
