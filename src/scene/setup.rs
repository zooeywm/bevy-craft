use bevy::prelude::*;
use bevy::ui::{AlignItems, BackgroundColor, JustifyContent, Node, PositionType, Val};

use crate::player::{FlyCamera, Player, PlayerBody, PlayerController, PreviewBlock, Velocity};
use crate::terrain::TerrainNoise;
use crate::voxel::{
    Block, InteractionCooldown, SelectedBlock, WorldState, build_single_block_mesh,
};
use crate::{BLOCK_SIZE, SHADOW_MAP_SIZE, STAND_EYE_HEIGHT, STAND_HALF_SIZE};

use crate::scene::SunBillboard;
use crate::scene::effects::SunVisualFactory;

/// Spawn block X coordinate used for initial player placement.
const PLAYER_SPAWN_X_BLOCK: i32 = 4;
/// Spawn block Z coordinate used for initial player placement.
const PLAYER_SPAWN_Z_BLOCK: i32 = 4;
/// Initial world position of the in-hand preview block.
const PREVIEW_SPAWN_POS: Vec3 = Vec3::new(2.0, 2.0, 1.2);
/// Uniform scale of the in-hand preview block.
const PREVIEW_SPAWN_SCALE: f32 = 0.15;
/// World-space sun position used for light direction.
const SUN_POSITION: Vec3 = Vec3::new(60.0, 50.0, 60.0);
/// Distance of sun billboard from camera.
const SUN_BILLBOARD_DISTANCE: f32 = 200.0;
/// Directional-light illuminance used for the sun.
const SUN_ILLUMINANCE: f32 = 14_000.0;
/// Directional-light color used for the sun.
const SUN_COLOR: Color = Color::srgb(1.0, 0.97, 0.90);
/// Initial player jump speed.
const PLAYER_JUMP_SPEED: f32 = 10.4;
/// Base player move speed.
const PLAYER_MOVE_SPEED: f32 = 8.4;
/// First-person camera sensitivity.
const CAMERA_SENSITIVITY: f32 = 0.002;
/// Initial first-person camera pitch angle.
const CAMERA_INITIAL_PITCH: f32 = -0.35;
/// Initial first-person camera yaw angle.
const CAMERA_INITIAL_YAW: f32 = -2.3;
/// Clear-color used for the sky background.
const SKY_COLOR: Color = Color::srgb(0.52, 0.74, 0.88);
/// Global ambient-light color.
const AMBIENT_COLOR: Color = Color::srgb(0.72, 0.78, 0.90);
/// Global ambient-light brightness.
const AMBIENT_BRIGHTNESS: f32 = 3_600.0;
/// Crosshair outer horizontal/vertical line length in pixels.
const CROSSHAIR_OUTER_LEN: f32 = 16.0;
/// Crosshair outer line thickness in pixels.
const CROSSHAIR_OUTER_THICK: f32 = 3.0;
/// Crosshair inner horizontal/vertical line length in pixels.
const CROSSHAIR_INNER_LEN: f32 = 10.0;
/// Crosshair inner line thickness in pixels.
const CROSSHAIR_INNER_THICK: f32 = 2.0;

/// Build initial world, lighting, player, camera, preview, and UI.
pub fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
) {
    setup_environment(&mut commands);
    let material = build_world_material(&asset_server, &mut materials);
    commands.insert_resource(SelectedBlock::new(Block::dirt_with_grass()));
    commands.insert_resource(InteractionCooldown::new());
    spawn_initial_chunk_world(&mut commands, &mut meshes, material.clone());
    spawn_sun(&mut commands, &mut meshes, &mut materials, &mut images);
    spawn_player_and_camera(&mut commands);
    spawn_preview_block(&mut commands, &mut meshes, material);

    spawn_crosshair_ui(&mut commands);
}

/// Insert global background, ambient-light, and shadow-map resources.
fn setup_environment(commands: &mut Commands) {
    // Sky-like background color.
    commands.insert_resource(ClearColor(SKY_COLOR));
    // Global ambient light to avoid fully black backfaces.
    commands.insert_resource(bevy::light::GlobalAmbientLight {
        color: AMBIENT_COLOR,
        brightness: AMBIENT_BRIGHTNESS,
        affects_lightmapped_meshes: true,
    });
    // Reduce shadow map resolution to improve performance.
    commands.insert_resource(bevy::light::DirectionalLightShadowMap {
        size: SHADOW_MAP_SIZE,
    });
}

/// Build the shared textured material for chunks and preview mesh.
fn build_world_material(
    asset_server: &Res<AssetServer>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
) -> Handle<StandardMaterial> {
    // Shared material for world blocks.
    let atlas_handle: Handle<Image> = asset_server.load("textures/atlas.png");
    materials.add(bevy::pbr::StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(atlas_handle),
        perceptual_roughness: 0.85,
        metallic: 0.0,
        reflectance: 0.04,
        ..default()
    })
}

/// Spawn the initial origin chunk and insert `WorldState`.
fn spawn_initial_chunk_world(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    material: Handle<StandardMaterial>,
) {
    let mut world_state = WorldState::new(material);
    let spawn_coord = IVec3::new(0, 0, 0);
    world_state.ensure_chunk(commands, meshes, spawn_coord);
    world_state.center = spawn_coord;
    commands.insert_resource(world_state);
}

/// Spawn directional sun light and its billboard mesh.
fn spawn_sun(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    images: &mut ResMut<Assets<Image>>,
) {
    // Sun-like directional light.
    commands.spawn((
        bevy::light::DirectionalLight {
            illuminance: SUN_ILLUMINANCE,
            color: SUN_COLOR,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_translation(SUN_POSITION).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    let sun_texture = images.add(SunVisualFactory::build_texture(256));
    let sun_material = materials.add(bevy::pbr::StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(sun_texture),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        cull_mode: None,
        ..default()
    });
    let sun_mesh = meshes.add(SunVisualFactory::build_quad(20.0));
    commands.spawn((
        bevy::mesh::Mesh3d(sun_mesh),
        bevy::pbr::MeshMaterial3d(sun_material),
        Transform::from_translation(Vec3::ZERO),
        bevy::light::NotShadowCaster,
        SunBillboard::from_world_position(SUN_POSITION, SUN_BILLBOARD_DISTANCE),
    ));
}

/// Spawn the player body and first-person camera.
fn spawn_player_and_camera(commands: &mut Commands) {
    let spawn_pos = SpawnLayout::player_position();
    let player_entity = commands
        .spawn((
            PlayerBody,
            Transform::from_translation(spawn_pos),
            Velocity::default(),
            Player::new_standing(PLAYER_JUMP_SPEED, STAND_HALF_SIZE, STAND_EYE_HEIGHT),
            PlayerController::new(PLAYER_MOVE_SPEED),
        ))
        .id();

    // First-person camera.
    commands.spawn((
        bevy::camera::Camera3d::default(),
        Transform::from_translation(SpawnLayout::camera_position(spawn_pos)),
        FlyCamera::new(
            CAMERA_SENSITIVITY,
            CAMERA_INITIAL_PITCH,
            CAMERA_INITIAL_YAW,
            player_entity,
        ),
    ));
}

/// Spawn-layout calculator for player and camera initial placement.
struct SpawnLayout;

impl SpawnLayout {
    /// Compute the player world-space spawn position from terrain height.
    fn player_position() -> Vec3 {
        let ground_height = TerrainNoise::height_at(PLAYER_SPAWN_X_BLOCK, PLAYER_SPAWN_Z_BLOCK);
        let spawn_y = (ground_height as f32 + 2.0) * BLOCK_SIZE + STAND_HALF_SIZE.y;
        let spawn_x = (PLAYER_SPAWN_X_BLOCK as f32 + 0.5) * BLOCK_SIZE;
        let spawn_z = (PLAYER_SPAWN_Z_BLOCK as f32 + 0.5) * BLOCK_SIZE;
        Vec3::new(spawn_x, spawn_y, spawn_z)
    }

    /// Convert player spawn position to camera spawn using eye-height offset.
    fn camera_position(player_spawn: Vec3) -> Vec3 {
        player_spawn + Vec3::Y * (STAND_EYE_HEIGHT - STAND_HALF_SIZE.y)
    }
}

/// Spawn the in-hand preview block mesh.
fn spawn_preview_block(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    material: Handle<StandardMaterial>,
) {
    // Preview block shown near the camera.
    let preview_mesh = meshes.add(build_single_block_mesh(Block::dirt_with_grass()));
    commands.spawn((
        bevy::mesh::Mesh3d(preview_mesh),
        bevy::pbr::MeshMaterial3d(material),
        Transform::from_translation(PREVIEW_SPAWN_POS).with_scale(Vec3::splat(PREVIEW_SPAWN_SCALE)),
        PreviewBlock,
    ));
}

/// Build a fixed UI crosshair (white outline plus black core).
fn spawn_crosshair_ui(commands: &mut Commands) {
    let outer_len = Val::Px(CROSSHAIR_OUTER_LEN);
    let outer_thick = Val::Px(CROSSHAIR_OUTER_THICK);
    let inner_len = Val::Px(CROSSHAIR_INNER_LEN);
    let inner_thick = Val::Px(CROSSHAIR_INNER_THICK);

    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::NONE),
        ))
        .with_children(|parent| {
            // White outline lines.
            parent.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: outer_len,
                    height: outer_thick,
                    ..default()
                },
                BackgroundColor(Color::WHITE),
            ));
            parent.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: outer_thick,
                    height: outer_len,
                    ..default()
                },
                BackgroundColor(Color::WHITE),
            ));

            // Black core lines.
            parent.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: inner_len,
                    height: inner_thick,
                    ..default()
                },
                BackgroundColor(Color::BLACK),
            ));
            parent.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: inner_thick,
                    height: inner_len,
                    ..default()
                },
                BackgroundColor(Color::BLACK),
            ));
        });
}

/// Lock and hide cursor for mouse-look controls.
pub fn setup_cursor(
    mut windows: Query<&mut bevy::window::CursorOptions, With<bevy::window::PrimaryWindow>>,
) {
    let Ok(mut cursor_options) = windows.single_mut() else {
        return;
    };
    cursor_options.grab_mode = bevy::window::CursorGrabMode::Locked;
    cursor_options.visible = false;
}
