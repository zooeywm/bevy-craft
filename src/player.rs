use bevy::prelude::*;

use crate::voxel::{WorldState, is_solid_world};
use crate::{
    CROUCH_EYE_HEIGHT, CROUCH_HALF_SIZE, CROUCH_TRANSITION_SPEED, GRAVITY, JUMP_BOOST_ACCEL,
    JUMP_BOOST_DURATION, STAND_EYE_HEIGHT, STAND_HALF_SIZE,
};

#[derive(Component)]
pub struct FlyCamera {
    pub sensitivity: f32,
    pub pitch: f32,
    pub yaw: f32,
    pub target: Entity,
}

#[derive(Component)]
pub struct PlayerBody;

#[derive(Component)]
pub struct Player {
    pub on_ground: bool,
    pub jump_speed: f32,
    pub jump_boost_time: f32,
    pub half_size: Vec3,
    pub eye_height: f32,
    pub target_half_size: Vec3,
    pub target_eye_height: f32,
    pub crouching: bool,
    pub flying: bool,
}

#[derive(Component)]
pub struct PlayerController {
    pub speed: f32,
}

#[derive(Component, Default)]
pub struct Velocity(pub Vec3);

// Update camera rotation based on mouse input and rotate the player body yaw.
pub fn camera_look_system(
    mouse_motion: Res<bevy::input::mouse::AccumulatedMouseMotion>,
    mut camera_query: Query<(&mut Transform, &mut FlyCamera), Without<PlayerBody>>,
    mut body_query: Query<&mut Transform, With<PlayerBody>>,
) {
    for (mut cam_transform, mut camera) in &mut camera_query {
        let delta = mouse_motion.delta;
        camera.yaw -= delta.x * camera.sensitivity;
        camera.pitch -= delta.y * camera.sensitivity;
        camera.pitch = camera.pitch.clamp(-1.55, 1.55);

        if let Ok(mut body_transform) = body_query.get_mut(camera.target) {
            body_transform.rotation = Quat::from_euler(EulerRot::YXZ, camera.yaw, 0.0, 0.0);
        }
        cam_transform.rotation = Quat::from_euler(EulerRot::YXZ, camera.yaw, camera.pitch, 0.0);
    }
}

// Keep the camera positioned at the player's eye height.
#[allow(clippy::type_complexity)]
pub fn camera_follow_system(
    mut camera_query: Query<(&mut Transform, &FlyCamera), Without<PlayerBody>>,
    body_query: Query<(&Transform, &Player), (With<PlayerBody>, Without<FlyCamera>)>,
) {
    for (mut cam_transform, camera) in &mut camera_query {
        if let Ok((body_transform, player)) = body_query.get(camera.target) {
            let eye_offset = player.eye_height - player.half_size.y;
            cam_transform.translation = body_transform.translation + Vec3::Y * eye_offset;
        }
    }
}

// Handle player movement input and apply ground/air movement or fly movement.
pub fn camera_move_system(
    input: Res<ButtonInput<KeyCode>>,
    mut query: Query<(&Transform, &PlayerController, &mut Velocity, &mut Player), With<PlayerBody>>,
) {
    for (transform, controller, mut velocity, mut player) in &mut query {
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

        // Flying mode: full 3D movement, no gravity or jump boost.
        if player.flying {
            if input.pressed(KeyCode::Space) {
                direction.y += 1.0;
            }
            if input.pressed(KeyCode::ControlLeft) {
                direction.y -= 1.0;
            }
            let mut wish = direction;
            if wish != Vec3::ZERO {
                let mut speed = controller.speed * 5.0;
                if input.pressed(KeyCode::ShiftLeft) {
                    speed *= 1.5;
                }
                wish = wish.normalize() * speed;
            }
            velocity.0 = wish;
            player.jump_boost_time = 0.0;
        } else {
            let mut wish = direction;
            wish.y = 0.0;
            if wish != Vec3::ZERO {
                let mut speed = controller.speed;
                if input.pressed(KeyCode::ShiftLeft) {
                    speed *= 1.5;
                }
                wish = wish.normalize() * speed;
            }
            // Ground movement snaps to desired horizontal speed.
            if player.on_ground {
                velocity.0.x = wish.x;
                velocity.0.z = wish.z;
            } else if wish != Vec3::ZERO {
                // Air control nudges velocity toward desired direction.
                let air_control = 0.08;
                velocity.0.x += (wish.x - velocity.0.x) * air_control;
                velocity.0.z += (wish.z - velocity.0.z) * air_control;
            }

            if input.just_pressed(KeyCode::Space) && player.on_ground {
                velocity.0.y = player.jump_speed;
                player.jump_boost_time = JUMP_BOOST_DURATION;
                player.on_ground = false;
            }
        }
    }
}

// Toggle flying mode with F2.
pub fn toggle_fly_system(
    input: Res<ButtonInput<KeyCode>>,
    mut query: Query<&mut Player, With<PlayerBody>>,
) {
    if !input.just_pressed(KeyCode::F2) {
        return;
    }
    for mut player in &mut query {
        player.flying = !player.flying;
        if player.flying {
            player.on_ground = false;
            player.jump_boost_time = 0.0;
        }
    }
}

// Start/stop crouch intent and update target collider/eye height.
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
                player.crouching = true;
                player.target_half_size = CROUCH_HALF_SIZE;
                player.target_eye_height = CROUCH_EYE_HEIGHT;
            }
        } else if player.crouching {
            let stand_pos =
                transform.translation + Vec3::Y * (STAND_HALF_SIZE.y - CROUCH_HALF_SIZE.y);
            if !intersects_solid(stand_pos, STAND_HALF_SIZE, &world) {
                player.crouching = false;
                player.target_half_size = STAND_HALF_SIZE;
                player.target_eye_height = STAND_EYE_HEIGHT;
            }
        }
    }
}

// Smoothly transition collider height and eye height for crouching.
pub fn crouch_transition_system(
    time: Res<Time>,
    mut query: Query<(&mut Transform, &mut Player), With<PlayerBody>>,
    world: Res<WorldState>,
) {
    let dt = time.delta_secs();
    let t = (CROUCH_TRANSITION_SPEED * dt).clamp(0.0, 1.0);
    for (mut transform, mut player) in &mut query {
        let old_half = player.half_size;
        let target = player.target_half_size;
        let new_y = old_half.y + (target.y - old_half.y) * t;
        if (new_y - old_half.y).abs() > f32::EPSILON {
            let candidate_pos = transform.translation + Vec3::Y * (new_y - old_half.y);
            let candidate_half = Vec3::new(old_half.x, new_y, old_half.z);
            // Allow shrinking always, but only allow growth if space is clear.
            if new_y <= old_half.y || !intersects_solid(candidate_pos, candidate_half, &world) {
                transform.translation = candidate_pos;
                player.half_size = candidate_half;
            }
        }

        player.eye_height += (player.target_eye_height - player.eye_height) * t;
    }
}

// Apply gravity, jump boost, and resolve movement with collision.
pub fn physics_system(
    time: Res<Time>,
    input: Res<ButtonInput<KeyCode>>,
    mut query: Query<(&mut Transform, &mut Velocity, &mut Player), With<PlayerBody>>,
    world: Res<WorldState>,
) {
    let dt = time.delta_secs();
    for (mut transform, mut velocity, mut player) in &mut query {
        // Only apply gravity/jump boost when not flying.
        if !player.flying {
            if !input.pressed(KeyCode::Space) {
                player.jump_boost_time = 0.0;
            }
            if player.jump_boost_time > 0.0 {
                velocity.0.y += JUMP_BOOST_ACCEL * dt;
                player.jump_boost_time -= dt;
            }

            velocity.0.y -= GRAVITY * dt;
        }

        let mut pos = transform.translation;
        let mut vel = velocity.0;
        player.on_ground = false;

        // Resolve collisions per axis to keep movement stable.
        move_axis(Vec3::X, &mut pos, &mut vel, player.half_size, &world, dt);
        move_axis(Vec3::Z, &mut pos, &mut vel, player.half_size, &world, dt);
        move_axis(Vec3::Y, &mut pos, &mut vel, player.half_size, &world, dt);

        if !player.flying && vel.y == 0.0 && velocity.0.y < 0.0 {
            player.on_ground = true;
        }

        transform.translation = pos;
        velocity.0 = vel;
    }
}

// Move along a single axis and stop on collision.
fn move_axis(
    axis: Vec3,
    pos: &mut Vec3,
    vel: &mut Vec3,
    half_size: Vec3,
    world: &WorldState,
    dt: f32,
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

    if intersects_solid(candidate, half_size, world) {
        if axis == Vec3::X {
            vel.x = 0.0;
        } else if axis == Vec3::Y {
            vel.y = 0.0;
        } else {
            vel.z = 0.0;
        }
        return;
    }
    *pos = candidate;
}

// Check whether a player-sized AABB intersects any solid block.
fn intersects_solid(position: Vec3, half_size: Vec3, world: &WorldState) -> bool {
    let min = position - half_size;
    let max = position + half_size;

    let min_x = (min.x / crate::BLOCK_SIZE).floor() as i32;
    let max_x = (max.x / crate::BLOCK_SIZE).floor() as i32;
    let min_y = (min.y / crate::BLOCK_SIZE).floor() as i32;
    let max_y = (max.y / crate::BLOCK_SIZE).floor() as i32;
    let min_z = (min.z / crate::BLOCK_SIZE).floor() as i32;
    let max_z = (max.z / crate::BLOCK_SIZE).floor() as i32;

    for z in min_z..=max_z {
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                if is_solid_world(IVec3::new(x, y, z), world) {
                    return true;
                }
            }
        }
    }
    false
}
