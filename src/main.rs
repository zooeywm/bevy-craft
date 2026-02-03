use bevy::prelude::*;

mod player;
mod terrain;
mod voxel;

use player::{
    FlyCamera, Player, PlayerBody, PlayerController, Velocity, camera_follow_system,
    camera_look_system, camera_move_system, crouch_system, crouch_transition_system,
    physics_system,
};
use terrain::height_at;
use voxel::{
    Block, Chunk, Crosshair, InteractionCooldown, PreviewBlock, SelectedBlock, WorldState,
    block_interaction_system, build_chunk_mesh, build_single_block_mesh, chunk_loading_system,
};

const CHUNK_SIZE: i32 = 16;
const BLOCK_SIZE: f32 = 0.5;
const VIEW_DISTANCE: i32 = 8;
const LOADS_PER_FRAME: usize = 2;
const MAX_IN_FLIGHT: usize = 8;
const GRAVITY: f32 = 20.0;
const JUMP_BOOST_DURATION: f32 = 0.12;
const JUMP_BOOST_ACCEL: f32 = 18.0;
const CROUCH_TRANSITION_SPEED: f32 = 12.0;
const STAND_HALF_SIZE: Vec3 = Vec3::new(0.3 * BLOCK_SIZE, 0.95 * BLOCK_SIZE, 0.3 * BLOCK_SIZE);
const CROUCH_HALF_SIZE: Vec3 = Vec3::new(0.3 * BLOCK_SIZE, 0.45 * BLOCK_SIZE, 0.3 * BLOCK_SIZE);
const STAND_EYE_HEIGHT: f32 = 1.8 * BLOCK_SIZE;
const CROUCH_EYE_HEIGHT: f32 = 0.8 * BLOCK_SIZE;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_systems(Startup, (setup_scene, setup_cursor))
        .add_systems(
            Update,
            (
                chunk_loading_system,
                camera_look_system,
                camera_move_system,
                crouch_system,
                crouch_transition_system,
                physics_system,
                camera_follow_system,
                block_interaction_system,
            ),
        )
        .add_systems(PostUpdate, (preview_follow_system, crosshair_follow_system))
        .run();
}

fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.insert_resource(ClearColor(Color::srgb(0.6, 0.75, 0.95)));
    commands.insert_resource(bevy::light::GlobalAmbientLight {
        color: Color::srgb(0.8, 0.8, 0.8),
        brightness: 80.0,
        affects_lightmapped_meshes: true,
    });
    let material = materials.add(bevy::pbr::StandardMaterial {
        base_color: Color::srgb(0.8, 0.4, 0.2),
        perceptual_roughness: 1.0,
        metallic: 0.0,
        reflectance: 0.0,
        ..default()
    });
    let preview_material_handle = material.clone();
    let crosshair_material = materials.add(bevy::pbr::StandardMaterial {
        base_color: Color::WHITE,
        unlit: true,
        ..default()
    });
    let mut world_state = WorldState {
        chunks: std::collections::HashMap::new(),
        material,
        center: IVec3::new(i32::MIN, i32::MIN, i32::MIN),
        needed: std::collections::HashSet::new(),
        pending: std::collections::VecDeque::new(),
        in_flight: std::collections::HashMap::new(),
    };
    commands.insert_resource(SelectedBlock {
        current: Block::Grass,
    });
    commands.insert_resource(InteractionCooldown {
        last_break_time: -1.0,
        last_place_time: -1.0,
    });

    let spawn_coord = IVec3::new(0, 0, 0);
    let spawn_chunk = Chunk::new_terrain(spawn_coord);
    let spawn_mesh = meshes.add(build_chunk_mesh(&spawn_chunk));
    let spawn_entity = commands
        .spawn((
            bevy::mesh::Mesh3d(spawn_mesh.clone()),
            bevy::pbr::MeshMaterial3d(world_state.material.clone()),
            Transform::from_xyz(
                spawn_coord.x as f32 * CHUNK_SIZE as f32 * BLOCK_SIZE,
                spawn_coord.y as f32 * CHUNK_SIZE as f32 * BLOCK_SIZE,
                spawn_coord.z as f32 * CHUNK_SIZE as f32 * BLOCK_SIZE,
            ),
        ))
        .id();
    world_state.chunks.insert(
        spawn_coord,
        voxel::ChunkData {
            chunk: spawn_chunk,
            mesh: spawn_mesh,
            entity: spawn_entity,
        },
    );
    world_state.center = spawn_coord;
    commands.insert_resource(world_state);

    // Light
    commands.spawn((
        bevy::light::DirectionalLight {
            illuminance: 80_000.0,
            color: Color::srgb(1.0, 1.0, 1.0),
            ..default()
        },
        Transform::from_xyz(5.0, 8.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        bevy::light::DirectionalLight {
            illuminance: 12_000.0,
            color: Color::srgb(0.9, 0.95, 1.0),
            ..default()
        },
        Transform::from_xyz(-6.0, 4.0, -4.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // Player body
    let spawn_x_block = 4;
    let spawn_z_block = 4;
    let ground_height = height_at(spawn_x_block, spawn_z_block);
    let spawn_y = (ground_height as f32 + 1.0) * BLOCK_SIZE + STAND_HALF_SIZE.y;
    let spawn_x = (spawn_x_block as f32 + 0.5) * BLOCK_SIZE;
    let spawn_z = (spawn_z_block as f32 + 0.5) * BLOCK_SIZE;
    let player_entity = commands
        .spawn((
            PlayerBody,
            Transform::from_xyz(spawn_x, spawn_y, spawn_z),
            Velocity::default(),
            Player {
                on_ground: false,
                jump_speed: 5.2,
                jump_boost_time: 0.0,
                half_size: STAND_HALF_SIZE,
                eye_height: STAND_EYE_HEIGHT,
                target_half_size: STAND_HALF_SIZE,
                target_eye_height: STAND_EYE_HEIGHT,
                crouching: false,
            },
            PlayerController { speed: 4.2 },
        ))
        .id();

    // Camera
    commands.spawn((
        bevy::camera::Camera3d::default(),
        Transform::from_xyz(
            spawn_x,
            spawn_y + (STAND_EYE_HEIGHT - STAND_HALF_SIZE.y),
            spawn_z,
        ),
        FlyCamera {
            speed: 10.0,
            sensitivity: 0.002,
            pitch: -0.35,
            yaw: -2.3,
            target: player_entity,
        },
    ));

    // Preview block
    let preview_mesh = meshes.add(build_single_block_mesh(Block::Grass));
    commands.spawn((
        bevy::mesh::Mesh3d(preview_mesh),
        bevy::pbr::MeshMaterial3d(preview_material_handle.clone()),
        Transform::from_xyz(2.0, 2.0, 1.2).with_scale(Vec3::splat(0.15)),
        PreviewBlock,
    ));

    // Crosshair (3D)
    let mut crosshair = Mesh::new(
        bevy::render::render_resource::PrimitiveTopology::LineList,
        bevy::asset::RenderAssetUsages::default(),
    );
    let size = 0.05 * BLOCK_SIZE;
    let positions = vec![
        [-size, 0.0, 0.0],
        [size, 0.0, 0.0],
        [0.0, -size, 0.0],
        [0.0, size, 0.0],
    ];
    let indices = vec![0, 1, 2, 3];
    crosshair.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    crosshair.insert_indices(bevy::mesh::Indices::U32(indices));
    let crosshair_mesh = meshes.add(crosshair);
    commands.spawn((
        bevy::mesh::Mesh3d(crosshair_mesh),
        bevy::pbr::MeshMaterial3d(crosshair_material),
        Transform::from_xyz(2.0, 2.0, 1.2),
        Crosshair,
    ));
}

fn setup_cursor(
    mut windows: Query<&mut bevy::window::CursorOptions, With<bevy::window::PrimaryWindow>>,
) {
    let Ok(mut cursor_options) = windows.single_mut() else {
        return;
    };
    cursor_options.grab_mode = bevy::window::CursorGrabMode::Locked;
    cursor_options.visible = false;
}

fn preview_follow_system(
    camera_query: Query<&Transform, (With<FlyCamera>, Without<PreviewBlock>)>,
    mut preview_query: Query<&mut Transform, (With<PreviewBlock>, Without<FlyCamera>)>,
) {
    let Ok(camera_transform) = camera_query.single() else {
        return;
    };
    let Ok(mut preview_transform) = preview_query.single_mut() else {
        return;
    };
    let forward: Vec3 = camera_transform.forward().as_vec3();
    let right: Vec3 = camera_transform.right().as_vec3();
    let translation =
        camera_transform.translation + forward * (BLOCK_SIZE * 2.0) + right * (BLOCK_SIZE * 0.8)
            - Vec3::Y * (BLOCK_SIZE * 0.6);
    preview_transform.translation = translation;
    preview_transform.rotation = camera_transform.rotation;
}

fn crosshair_follow_system(
    camera_query: Query<&Transform, (With<FlyCamera>, Without<Crosshair>)>,
    mut crosshair_query: Query<&mut Transform, (With<Crosshair>, Without<FlyCamera>)>,
) {
    let Ok(camera_transform) = camera_query.single() else {
        return;
    };
    let Ok(mut crosshair_transform) = crosshair_query.single_mut() else {
        return;
    };
    let forward: Vec3 = camera_transform.forward().as_vec3();
    crosshair_transform.translation = camera_transform.translation + forward * (BLOCK_SIZE * 1.5);
    crosshair_transform.rotation = camera_transform.rotation;
}
