use bevy::prelude::*;

use crate::voxel::WorldState;
use crate::{
    CROUCH_EYE_HEIGHT, CROUCH_HALF_SIZE, CROUCH_TRANSITION_SPEED, GRAVITY, JUMP_BOOST_ACCEL,
    STAND_EYE_HEIGHT, STAND_HALF_SIZE,
};

use crate::player::components::{Player, PlayerBody, Velocity};

/// Start or stop crouch intent and update target collider/eye height.
pub fn crouch_system(
    input: Res<ButtonInput<KeyCode>>,
    mut query: Query<(&mut Transform, &mut Player), With<PlayerBody>>,
    world: Res<WorldState>,
) {
    for (transform, mut player) in &mut query {
        if player.flying {
            continue;
        }
        if input.pressed(KeyCode::ControlLeft) {
            if !player.crouching {
                player.enter_crouch(CROUCH_HALF_SIZE, CROUCH_EYE_HEIGHT);
            }
        } else if player.crouching {
            let stand_pos =
                transform.translation + Vec3::Y * (STAND_HALF_SIZE.y - CROUCH_HALF_SIZE.y);
            if !world.intersects_solid(stand_pos, STAND_HALF_SIZE) {
                player.exit_crouch(STAND_HALF_SIZE, STAND_EYE_HEIGHT);
            }
        }
    }
}

/// Smoothly transition collider and eye-height state for crouching.
pub fn crouch_transition_system(
    time: Res<Time>,
    mut query: Query<(&mut Transform, &mut Player), With<PlayerBody>>,
    world: Res<WorldState>,
) {
    let dt = time.delta_secs();
    let t = (CROUCH_TRANSITION_SPEED * dt).clamp(0.0, 1.0);
    for (mut transform, mut player) in &mut query {
        player.apply_crouch_transition(&mut transform, &world, t);
    }
}

/// Apply gravity and movement, then resolve collisions.
pub fn physics_system(
    time: Res<Time>,
    input: Res<ButtonInput<KeyCode>>,
    mut query: Query<(&mut Transform, &mut Velocity, &mut Player), With<PlayerBody>>,
    world: Res<WorldState>,
) {
    let dt = time.delta_secs();
    for (mut transform, mut velocity, mut player) in &mut query {
        let was_on_ground = player.on_ground;
        // Only apply gravity/jump boost when not flying.
        if !player.flying {
            player.apply_vertical_forces(
                &mut velocity.0,
                input.pressed(KeyCode::Space),
                dt,
                JUMP_BOOST_ACCEL,
                GRAVITY,
            );
        }

        let mut pos = transform.translation;
        let mut vel = velocity.0;
        player.on_ground = false;
        let crouch_edge_guard = player.crouch_edge_guard_enabled(was_on_ground);

        // Resolve collisions per axis to keep movement stable.
        player.resolve_motion_axes(&mut pos, &mut vel, &world, dt, crouch_edge_guard);

        let was_flying = player.flying;
        let old_vertical_velocity = velocity.0.y;
        player.update_grounded_after_move(was_flying, old_vertical_velocity, vel.y);

        transform.translation = pos;
        velocity.0 = vel;
    }
}

#[cfg(test)]
mod tests {
    use bevy::prelude::*;

    use super::*;

    /// Verify crouch edge guard prevents horizontal movement without ground support.
    #[test]
    fn crouch_edge_guard_blocks_horizontal_movement_when_unsupported() {
        let world = WorldState::new(Handle::<StandardMaterial>::default());
        let player = Player::new_standing(10.0, STAND_HALF_SIZE, STAND_EYE_HEIGHT);

        let mut guarded_pos = Vec3::new(1.5, 2.0, 1.5);
        let mut guarded_vel = Vec3::new(4.0, 0.0, 0.0);
        player.resolve_motion_axes(&mut guarded_pos, &mut guarded_vel, &world, 0.1, true);
        assert_eq!(guarded_pos, Vec3::new(1.5, 2.0, 1.5));
        assert_eq!(guarded_vel.x, 0.0);

        let mut free_pos = Vec3::new(1.5, 2.0, 1.5);
        let mut free_vel = Vec3::new(4.0, 0.0, 0.0);
        player.resolve_motion_axes(&mut free_pos, &mut free_vel, &world, 0.1, false);
        assert!(free_pos.x > 1.5);
    }
}
