use bevy::prelude::*;

use crate::{BLOCK_SIZE, JUMP_BOOST_DURATION};
use crate::voxel::Block;
use crate::voxel::WorldState;

/// Camera controller state used by first-person look and follow systems.
#[derive(Component)]
pub struct FlyCamera {
    /// Mouse-look sensitivity factor.
    pub sensitivity: f32,
    /// Pitch angle in radians.
    pub pitch: f32,
    /// Yaw angle in radians.
    pub yaw: f32,
    /// Target player entity followed by this camera.
    pub target: Entity,
}

impl FlyCamera {
    /// Minimum pitch angle clamp for first-person look.
    const PITCH_MIN: f32 = -1.55;
    /// Maximum pitch angle clamp for first-person look.
    const PITCH_MAX: f32 = 1.55;

    /// Apply mouse delta to yaw/pitch with sensitivity and clamp pitch.
    pub fn apply_mouse_look(&mut self, delta: Vec2) {
        self.yaw -= delta.x * self.sensitivity;
        self.pitch -= delta.y * self.sensitivity;
        self.pitch = self.pitch.clamp(Self::PITCH_MIN, Self::PITCH_MAX);
    }

    /// Build body rotation from camera yaw only.
    pub fn body_rotation(&self) -> Quat {
        Quat::from_euler(EulerRot::YXZ, self.yaw, 0.0, 0.0)
    }

    /// Build camera rotation from yaw and pitch.
    pub fn camera_rotation(&self) -> Quat {
        Quat::from_euler(EulerRot::YXZ, self.yaw, self.pitch, 0.0)
    }

    /// Build first-person camera controller state.
    pub fn new(sensitivity: f32, pitch: f32, yaw: f32, target: Entity) -> Self {
        Self {
            sensitivity,
            pitch,
            yaw,
            target,
        }
    }

    /// Compute camera world position from player body position and eye offset.
    pub fn follow_translation(&self, body_position: Vec3, player: &Player) -> Vec3 {
        player.eye_world_position(body_position)
    }
}

/// Marker component for the physics-driven player body entity.
#[derive(Component)]
pub struct PlayerBody;

/// Runtime state for player locomotion and stance.
#[derive(Component)]
pub struct Player {
    /// Whether the player is currently grounded.
    pub on_ground: bool,
    /// Initial jump impulse speed.
    pub jump_speed: f32,
    /// Remaining jump-boost time while jump is held.
    pub jump_boost_time: f32,
    /// Current half-size of the collider AABB.
    pub half_size: Vec3,
    /// Current camera eye height in world units.
    pub eye_height: f32,
    /// Target collider half-size for crouch transitions.
    pub target_half_size: Vec3,
    /// Target eye height for crouch transitions.
    pub target_eye_height: f32,
    /// Whether the player is currently crouching.
    pub crouching: bool,
    /// Whether the player is currently in fly mode.
    pub flying: bool,
}

impl Player {
    /// Air-control interpolation factor used while in the air.
    const AIR_CONTROL: f32 = 0.08;

    /// Build default standing player state for initial spawn.
    pub fn new_standing(jump_speed: f32, half_size: Vec3, eye_height: f32) -> Self {
        Self {
            on_ground: false,
            jump_speed,
            jump_boost_time: 0.0,
            half_size,
            eye_height,
            target_half_size: half_size,
            target_eye_height: eye_height,
            crouching: false,
            flying: false,
        }
    }

    /// Toggle fly mode and reset ground/jump-boost state when enabling flight.
    pub fn toggle_flying(&mut self) {
        self.flying = !self.flying;
        if self.flying {
            self.on_ground = false;
            self.jump_boost_time = 0.0;
        }
    }

    /// Handle fly-toggle hotkey and apply toggle when key is just pressed.
    pub fn handle_fly_toggle_hotkey(&mut self, input: &ButtonInput<KeyCode>) {
        if input.just_pressed(KeyCode::F2) {
            self.toggle_flying();
        }
    }

    /// Start a jump from grounded state and apply jump boost duration.
    pub fn try_start_jump(&mut self, velocity: &mut Vec3) {
        if !self.on_ground {
            return;
        }
        velocity.y = self.jump_speed;
        self.jump_boost_time = JUMP_BOOST_DURATION;
        self.on_ground = false;
    }

    /// Return current eye offset from player-body origin.
    pub fn eye_offset(&self) -> f32 {
        self.eye_height - self.half_size.y
    }

    /// Return world-space eye position from player-body world position.
    pub fn eye_world_position(&self, body_position: Vec3) -> Vec3 {
        body_position + Vec3::Y * self.eye_offset()
    }

    /// Enter crouch state and set crouch collider/eye targets.
    pub fn enter_crouch(&mut self, crouch_half_size: Vec3, crouch_eye_height: f32) {
        self.crouching = true;
        self.target_half_size = crouch_half_size;
        self.target_eye_height = crouch_eye_height;
    }

    /// Exit crouch state and restore standing collider/eye targets.
    pub fn exit_crouch(&mut self, stand_half_size: Vec3, stand_eye_height: f32) {
        self.crouching = false;
        self.target_half_size = stand_half_size;
        self.target_eye_height = stand_eye_height;
    }

    /// Return whether player AABB at `player_pos` overlaps target block AABB.
    pub fn intersects_block(&self, player_pos: Vec3, block_world: IVec3) -> bool {
        let block_min = Block::world_translation(block_world);
        let block_max = block_min + Vec3::splat(BLOCK_SIZE);

        let player_min = player_pos - self.half_size;
        let player_max = player_pos + self.half_size;

        player_min.x < block_max.x
            && player_max.x > block_min.x
            && player_min.y < block_max.y
            && player_max.y > block_min.y
            && player_min.z < block_max.z
            && player_max.z > block_min.z
    }

    /// Update jump-boost timer and apply extra vertical acceleration when active.
    pub fn apply_jump_boost(
        &mut self,
        velocity: &mut Vec3,
        jump_pressed: bool,
        dt: f32,
        jump_boost_accel: f32,
    ) {
        if !jump_pressed {
            self.jump_boost_time = 0.0;
        }
        if self.jump_boost_time <= 0.0 {
            return;
        }
        velocity.y += jump_boost_accel * dt;
        self.jump_boost_time -= dt;
    }

    /// Apply vertical forces for one frame: jump boost first, then gravity.
    pub fn apply_vertical_forces(
        &mut self,
        velocity: &mut Vec3,
        jump_pressed: bool,
        dt: f32,
        jump_boost_accel: f32,
        gravity: f32,
    ) {
        self.apply_jump_boost(velocity, jump_pressed, dt, jump_boost_accel);
        velocity.y -= gravity * dt;
    }

    /// Update grounded flag after axis-resolved physics step.
    pub fn update_grounded_after_move(
        &mut self,
        was_flying: bool,
        old_vertical_velocity: f32,
        resolved_vertical_velocity: f32,
    ) {
        if !was_flying && resolved_vertical_velocity == 0.0 && old_vertical_velocity < 0.0 {
            self.on_ground = true;
        }
    }

    /// Return whether crouch edge guard should be enabled this frame.
    pub fn crouch_edge_guard_enabled(&self, was_on_ground: bool) -> bool {
        self.crouching && !self.flying && was_on_ground
    }

    /// Apply horizontal velocity from desired wish vector on ground or in air.
    pub fn apply_horizontal_movement(&self, velocity: &mut Vec3, wish: Vec3) {
        if self.on_ground {
            velocity.x = wish.x;
            velocity.z = wish.z;
            return;
        }
        if wish == Vec3::ZERO {
            return;
        }
        velocity.x += (wish.x - velocity.x) * Self::AIR_CONTROL;
        velocity.z += (wish.z - velocity.z) * Self::AIR_CONTROL;
    }

    /// Resolve movement against voxel collisions in X/Z then Y order.
    pub(crate) fn resolve_motion_axes(
        &self,
        pos: &mut Vec3,
        vel: &mut Vec3,
        world: &WorldState,
        dt: f32,
        crouch_edge_guard: bool,
    ) {
        self.move_axis(Vec3::X, pos, vel, world, dt, crouch_edge_guard);
        self.move_axis(Vec3::Z, pos, vel, world, dt, crouch_edge_guard);
        self.move_axis(Vec3::Y, pos, vel, world, dt, false);
    }

    /// Apply one crouch-transition step to collider and eye height.
    pub(crate) fn apply_crouch_transition(
        &mut self,
        transform: &mut Transform,
        world: &WorldState,
        t: f32,
    ) {
        let old_half = self.half_size;
        let target = self.target_half_size;
        let new_y = old_half.y + (target.y - old_half.y) * t;
        if (new_y - old_half.y).abs() > f32::EPSILON {
            let candidate_pos = transform.translation + Vec3::Y * (new_y - old_half.y);
            let candidate_half = Vec3::new(old_half.x, new_y, old_half.z);
            // Allow shrinking always, but only allow growth if space is clear.
            if new_y <= old_half.y || !world.intersects_solid(candidate_pos, candidate_half) {
                transform.translation = candidate_pos;
                self.half_size = candidate_half;
            }
        }
        self.eye_height += (self.target_eye_height - self.eye_height) * t;
    }

    /// Move along one axis and clamp velocity on collision.
    fn move_axis(
        &self,
        axis: Vec3,
        pos: &mut Vec3,
        vel: &mut Vec3,
        world: &WorldState,
        dt: f32,
        prevent_fall: bool,
    ) {
        let delta = if axis == Vec3::X {
            vel.x * dt
        } else if axis == Vec3::Y {
            vel.y * dt
        } else {
            vel.z * dt
        };
        if delta == 0.0 {
            return;
        }

        let mut candidate = *pos;
        if axis == Vec3::X {
            candidate.x += delta;
        } else if axis == Vec3::Y {
            candidate.y += delta;
        } else {
            candidate.z += delta;
        }

        if world.intersects_solid(candidate, self.half_size) {
            if axis == Vec3::X {
                vel.x = 0.0;
            } else if axis == Vec3::Y {
                vel.y = 0.0;
            } else {
                vel.z = 0.0;
            }
            return;
        }

        if prevent_fall
            && axis != Vec3::Y
            && !world.has_ground_support(candidate, self.half_size)
        {
            if axis == Vec3::X {
                vel.x = 0.0;
            } else {
                vel.z = 0.0;
            }
            return;
        }

        *pos = candidate;
    }
}

/// Tunable movement controller parameters.
#[derive(Component)]
pub struct PlayerController {
    /// Base movement speed.
    pub speed: f32,
}

impl PlayerController {
    /// Speed multiplier used while sprint key is held.
    const SPRINT_MULTIPLIER: f32 = 1.5;
    /// Base speed multiplier applied in flying mode.
    const FLY_MULTIPLIER: f32 = 5.0;

    /// Compute current movement speed from stance and sprint state.
    pub fn move_speed(&self, flying: bool, sprinting: bool) -> f32 {
        let mut speed = self.speed;
        if flying {
            speed *= Self::FLY_MULTIPLIER;
        }
        if sprinting {
            speed *= Self::SPRINT_MULTIPLIER;
        }
        speed
    }

    /// Convert desired direction into final wish velocity.
    pub fn wish_velocity(&self, direction: Vec3, flying: bool, sprinting: bool) -> Vec3 {
        if direction == Vec3::ZERO {
            return Vec3::ZERO;
        }
        direction.normalize() * self.move_speed(flying, sprinting)
    }

    /// Build movement controller with base speed.
    pub fn new(speed: f32) -> Self {
        Self { speed }
    }

    /// Build desired movement direction from key input and camera basis.
    pub fn desired_direction(
        &self,
        input: &ButtonInput<KeyCode>,
        transform: &Transform,
        flying: bool,
    ) -> Vec3 {
        let mut direction = Vec3::ZERO;
        if input.pressed(KeyCode::KeyW) {
            direction += transform.forward().as_vec3();
        }
        if input.pressed(KeyCode::KeyS) {
            direction -= transform.forward().as_vec3();
        }
        if input.pressed(KeyCode::KeyA) {
            direction -= transform.right().as_vec3();
        }
        if input.pressed(KeyCode::KeyD) {
            direction += transform.right().as_vec3();
        }
        if flying {
            if input.pressed(KeyCode::Space) {
                direction.y += 1.0;
            }
            if input.pressed(KeyCode::ControlLeft) {
                direction.y -= 1.0;
            }
        } else {
            direction.y = 0.0;
        }
        direction
    }
}

/// Linear velocity component for player movement integration.
#[derive(Component, Default)]
pub struct Velocity(
    /// Current velocity in world units per second.
    pub Vec3,
);

#[cfg(test)]
mod tests {
    use bevy::prelude::{IVec3, Vec3};

    use super::Player;

    /// Ensure placement-collision guard blocks overlapping placement and allows clear placement.
    #[test]
    fn player_intersects_block_for_placement_guard() {
        let player = Player::new_standing(10.0, Vec3::new(0.3, 0.95, 0.3), 1.8);
        let player_pos = Vec3::new(1.5, 2.0, 1.5);

        // Same block column and overlapping AABB.
        assert!(player.intersects_block(player_pos, IVec3::new(1, 1, 1)));

        // Far away block should not overlap.
        assert!(!player.intersects_block(player_pos, IVec3::new(4, 1, 4)));
    }
}
