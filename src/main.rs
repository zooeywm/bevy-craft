use bevy::image::ImagePlugin;
use bevy::prelude::*;

mod player;
mod scene;
mod terrain;
mod voxel;
mod material_catalog;

use player::{
    camera_follow_system, camera_look_system, camera_move_system, crouch_system,
    crouch_transition_system, physics_system, preview_follow_system, toggle_fly_system,
};
use scene::{setup_cursor, setup_scene, sun_billboard_system};
use voxel::{
    BlockFallScanTimer, block_interaction_system, chunk_loading_system,
    spawn_falling_blocks_system, update_falling_blocks_system,
};

/// Chunk width/height/depth in blocks.
const CHUNK_SIZE: i32 = 16;
/// Size of one block in world units.
const BLOCK_SIZE: f32 = 1.0;
/// Horizontal chunk radius around the player to keep loaded.
const VIEW_DISTANCE: i32 = 10;
/// Number of vertical chunk layers to generate (y=0..layers-1).
const VERTICAL_CHUNK_LAYERS: i32 = 6;
/// Max chunk builds started per frame.
const LOADS_PER_FRAME: usize = 16;
/// Max async chunk build tasks in flight.
const MAX_IN_FLIGHT: usize = 16;
/// Gravity acceleration for the player.
const GRAVITY: f32 = 40.0;
/// Duration of jump boost when holding jump.
const JUMP_BOOST_DURATION: f32 = 0.12;
/// Upward acceleration during jump boost.
const JUMP_BOOST_ACCEL: f32 = 36.0;
/// Smoothing speed for crouch transitions.
const CROUCH_TRANSITION_SPEED: f32 = 12.0;
/// Half-size of the standing player collider.
const STAND_HALF_SIZE: Vec3 = Vec3::new(0.3 * BLOCK_SIZE, 0.95 * BLOCK_SIZE, 0.3 * BLOCK_SIZE);
/// Half-size of the crouching player collider.
const CROUCH_HALF_SIZE: Vec3 = Vec3::new(0.3 * BLOCK_SIZE, 0.45 * BLOCK_SIZE, 0.3 * BLOCK_SIZE);
/// Eye height when standing (in world units).
const STAND_EYE_HEIGHT: f32 = 1.8 * BLOCK_SIZE;
/// Eye height when crouching (in world units).
const CROUCH_EYE_HEIGHT: f32 = 0.8 * BLOCK_SIZE;
/// Shadow map resolution for directional light (lower = faster).
const SHADOW_MAP_SIZE: usize = 1024;

/// App entry point and system registration.
fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(ImagePlugin::default_nearest()))
        .insert_resource(BlockFallScanTimer::new(0.08))
        .add_systems(Startup, (setup_scene, setup_cursor))
        .add_systems(
            Update,
            (
                chunk_loading_system,
                camera_look_system,
                camera_move_system,
                toggle_fly_system,
                crouch_system,
                crouch_transition_system,
                physics_system,
                camera_follow_system,
                block_interaction_system,
                spawn_falling_blocks_system,
                update_falling_blocks_system,
            ),
        )
        .add_systems(PostUpdate, (preview_follow_system, sun_billboard_system))
        .run();
}
