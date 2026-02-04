use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::ui::{AlignItems, BackgroundColor, JustifyContent, Node, PositionType, Val};
use image::GenericImage;

mod player;
mod terrain;
mod voxel;

use player::{
    FlyCamera, Player, PlayerBody, PlayerController, Velocity, camera_follow_system,
    camera_look_system, camera_move_system, crouch_system, crouch_transition_system,
    physics_system, toggle_fly_system,
};
use terrain::height_at;
use voxel::{
    Block, BlockFallScanTimer, Chunk, InteractionCooldown, PreviewBlock, SelectedBlock, WorldState,
    block_interaction_system, build_chunk_mesh, build_single_block_mesh, chunk_loading_system,
    spawn_falling_blocks_system, update_falling_blocks_system,
};

// Chunk width/height/depth in blocks.
const CHUNK_SIZE: i32 = 16;
// Size of one block in world units.
const BLOCK_SIZE: f32 = 0.5;
// Horizontal chunk radius around the player to keep loaded.
const VIEW_DISTANCE: i32 = 10;
// Number of vertical chunk layers to generate (y=0..layers-1).
const VERTICAL_CHUNK_LAYERS: i32 = 6;
// Max chunk builds started per frame.
const LOADS_PER_FRAME: usize = 16;
// Max async chunk build tasks in flight.
const MAX_IN_FLIGHT: usize = 16;
// Gravity acceleration for the player.
const GRAVITY: f32 = 20.0;
// Duration of jump boost when holding jump.
const JUMP_BOOST_DURATION: f32 = 0.12;
// Upward acceleration during jump boost.
const JUMP_BOOST_ACCEL: f32 = 18.0;
// Smoothing speed for crouch transitions.
const CROUCH_TRANSITION_SPEED: f32 = 12.0;
// Half-size of the standing player collider.
const STAND_HALF_SIZE: Vec3 = Vec3::new(0.3 * BLOCK_SIZE, 0.95 * BLOCK_SIZE, 0.3 * BLOCK_SIZE);
// Half-size of the crouching player collider.
const CROUCH_HALF_SIZE: Vec3 = Vec3::new(0.3 * BLOCK_SIZE, 0.45 * BLOCK_SIZE, 0.3 * BLOCK_SIZE);
// Eye height when standing (in world units).
const STAND_EYE_HEIGHT: f32 = 1.8 * BLOCK_SIZE;
// Eye height when crouching (in world units).
const CROUCH_EYE_HEIGHT: f32 = 0.8 * BLOCK_SIZE;
// Shadow map resolution for directional light (lower = faster).
const SHADOW_MAP_SIZE: usize = 256;

// App entry point and system registration.
fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .insert_resource(BlockFallScanTimer(Timer::from_seconds(
            0.08,
            TimerMode::Repeating,
        )))
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

// Build the initial world, lighting, player, camera, and UI helpers.
fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    // Sky-like background color.
    commands.insert_resource(ClearColor(Color::srgb(0.6, 0.75, 0.95)));
    // Global ambient light to avoid fully black backfaces.
    commands.insert_resource(bevy::light::GlobalAmbientLight {
        color: Color::srgb(0.8, 0.8, 0.8),
        brightness: 10_000.0,
        affects_lightmapped_meshes: true,
    });
    // Reduce shadow map resolution to improve performance.
    commands.insert_resource(bevy::light::DirectionalLightShadowMap {
        size: SHADOW_MAP_SIZE,
    });
    // Shared material for world blocks.
    let atlas = build_texture_atlas();
    let atlas_handle = images.add(atlas);
    let material = materials.add(bevy::pbr::StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(atlas_handle.clone()),
        perceptual_roughness: 0.85,
        metallic: 0.0,
        reflectance: 0.04,
        ..default()
    });
    // Preview block uses the same material as the world.
    let preview_material_handle = material.clone();
    // Initialize world state and dynamic chunk tracking.
    let mut world_state = WorldState {
        chunks: std::collections::HashMap::new(),
        material,
        center: IVec3::new(i32::MIN, i32::MIN, i32::MIN),
        needed: std::collections::HashSet::new(),
        pending: std::collections::VecDeque::new(),
        in_flight: std::collections::HashMap::new(),
    };
    // Default selected block for placement.
    commands.insert_resource(SelectedBlock {
        current: Block::Grass,
    });
    // Cooldown timers for repeat place/break.
    commands.insert_resource(InteractionCooldown {
        last_break_time: -1.0,
        last_place_time: -1.0,
    });

    // Spawn the initial terrain chunk at the origin.
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

    // Sun-like directional light.
    let sun_position = Vec3::new(60.0, 50.0, 60.0);
    commands.spawn((
        bevy::light::DirectionalLight {
            illuminance: 30_000.0,
            color: Color::srgb(1.0, 0.98, 0.95),
            shadows_enabled: true,
            ..default()
        },
        Transform::from_translation(sun_position).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    let sun_texture = images.add(build_sun_texture(256));
    let sun_material = materials.add(bevy::pbr::StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(sun_texture),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        cull_mode: None,
        ..default()
    });
    let sun_mesh = meshes.add(build_sun_quad(20.0));
    commands.spawn((
        bevy::mesh::Mesh3d(sun_mesh),
        bevy::pbr::MeshMaterial3d(sun_material),
        Transform::from_translation(Vec3::ZERO),
        bevy::light::NotShadowCaster,
        SunBillboard {
            direction: (sun_position - Vec3::ZERO).normalize_or_zero(),
            distance: 200.0,
        },
    ));
    // No secondary fill light.

    // Player body spawn positioned above terrain.
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
                flying: false,
            },
            PlayerController { speed: 4.2 },
        ))
        .id();

    // First-person camera.
    commands.spawn((
        bevy::camera::Camera3d::default(),
        Transform::from_xyz(
            spawn_x,
            spawn_y + (STAND_EYE_HEIGHT - STAND_HALF_SIZE.y),
            spawn_z,
        ),
        FlyCamera {
            sensitivity: 0.002,
            pitch: -0.35,
            yaw: -2.3,
            target: player_entity,
        },
    ));

    // Preview block shown near the camera.
    let preview_mesh = meshes.add(build_single_block_mesh(Block::Grass));
    commands.spawn((
        bevy::mesh::Mesh3d(preview_mesh),
        bevy::pbr::MeshMaterial3d(preview_material_handle.clone()),
        Transform::from_xyz(2.0, 2.0, 1.2).with_scale(Vec3::splat(0.15)),
        PreviewBlock,
    ));

    spawn_crosshair_ui(&mut commands);
}

// Lock and hide cursor for mouse look.
fn setup_cursor(
    mut windows: Query<&mut bevy::window::CursorOptions, With<bevy::window::PrimaryWindow>>,
) {
    let Ok(mut cursor_options) = windows.single_mut() else {
        return;
    };
    cursor_options.grab_mode = bevy::window::CursorGrabMode::Locked;
    cursor_options.visible = false;
}

// Keep the preview block aligned to the camera.
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

// Build a fixed UI crosshair (white outline + black core).
fn spawn_crosshair_ui(commands: &mut Commands) {
    let outer_len = Val::Px(16.0);
    let outer_thick = Val::Px(3.0);
    let inner_len = Val::Px(10.0);
    let inner_thick = Val::Px(2.0);

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

#[derive(Component)]
struct SunBillboard {
    direction: Vec3,
    distance: f32,
}

// Keep the sun quad at a fixed direction relative to the camera.
fn sun_billboard_system(
    camera_query: Query<&Transform, (With<FlyCamera>, Without<SunBillboard>)>,
    mut sun_query: Query<(&SunBillboard, &mut Transform)>,
) {
    let Ok(camera_transform) = camera_query.single() else {
        return;
    };
    for (sun, mut transform) in &mut sun_query {
        transform.translation = camera_transform.translation + sun.direction * sun.distance;
        transform.look_at(camera_transform.translation, Vec3::Y);
    }
}

// Build a 1x3 texture atlas for grass side/top and dirt (pixel-art style).
fn build_texture_atlas() -> Image {
    let grass_side = load_rgba("assets/textures/grass.png");
    let grass_top = load_rgba("assets/textures/grasstop.png");
    let dirt = load_rgba("assets/textures/dirt.png");
    let (tile_w, tile_h) = grass_side.dimensions();

    let mut atlas = image::RgbaImage::new(tile_w * 3, tile_h);
    atlas.copy_from(&grass_side, 0, 0).expect("copy grass");
    atlas
        .copy_from(&grass_top, tile_w, 0)
        .expect("copy grass top");
    atlas.copy_from(&dirt, tile_w * 2, 0).expect("copy dirt");

    let size = Extent3d {
        width: atlas.width(),
        height: atlas.height(),
        depth_or_array_layers: 1,
    };
    let mut image = Image::new_fill(
        size,
        TextureDimension::D2,
        &[0, 0, 0, 0],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.data = Some(atlas.into_raw());
    image.sampler = ImageSampler::nearest();
    image
}

// Load a PNG file into RGBA8 image data.
fn load_rgba(path: &str) -> image::RgbaImage {
    let bytes = std::fs::read(path).expect("texture file not found");
    let img = image::load_from_memory(&bytes).expect("decode texture");
    img.to_rgba8()
}

// Build a circular sun texture with a soft edge.
fn build_sun_texture(size: u32) -> Image {
    let mut data = vec![0u8; (size * size * 4) as usize];
    let center = (size as f32 - 1.0) * 0.5;
    let radius = size as f32 * 0.4;
    let feather = size as f32 * 0.05;
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - center;
            let dy = y as f32 - center;
            let dist = (dx * dx + dy * dy).sqrt();
            let t = ((radius - dist) / feather).clamp(0.0, 1.0);
            let alpha = (t * t * (3.0 - 2.0 * t) * 255.0) as u8;
            let idx = ((y * size + x) * 4) as usize;
            data[idx] = 255;
            data[idx + 1] = 245;
            data[idx + 2] = 220;
            data[idx + 3] = alpha;
        }
    }
    let size = Extent3d {
        width: size,
        height: size,
        depth_or_array_layers: 1,
    };
    let mut image = Image::new_fill(
        size,
        TextureDimension::D2,
        &[0, 0, 0, 0],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.data = Some(data);
    image.sampler = ImageSampler::linear();
    image
}

// Build a simple quad mesh facing +Z.
fn build_sun_quad(size: f32) -> Mesh {
    let half = size * 0.5;
    let positions = vec![
        [-half, -half, 0.0],
        [half, -half, 0.0],
        [half, half, 0.0],
        [-half, half, 0.0],
    ];
    let normals = vec![[0.0, 0.0, 1.0]; 4];
    let uvs = vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let indices = vec![0u32, 1, 2, 0, 2, 3];
    let mut mesh = Mesh::new(
        bevy::render::render_resource::PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(bevy::mesh::Indices::U32(indices));
    mesh
}
