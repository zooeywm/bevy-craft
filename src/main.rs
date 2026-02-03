use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task};
use futures_lite::future;
use std::collections::{HashMap, HashSet, VecDeque};

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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Block {
    Air,
    Grass,
    Dirt,
}

struct Chunk {
    blocks: Vec<Block>,
}

#[derive(Resource)]
struct WorldState {
    chunks: HashMap<IVec3, ChunkData>,
    material: Handle<StandardMaterial>,
    center: IVec3,
    needed: HashSet<IVec3>,
    pending: VecDeque<IVec3>,
    in_flight: HashMap<IVec3, Task<ChunkBuildOutput>>,
}

struct ChunkData {
    chunk: Chunk,
    mesh: Handle<Mesh>,
    entity: Entity,
}

#[derive(Resource)]
struct SelectedBlock {
    current: Block,
}

#[derive(Resource)]
struct InteractionCooldown {
    last_break_time: f32,
    last_place_time: f32,
}

#[derive(Component)]
struct PreviewBlock;

#[derive(Component)]
struct Crosshair;

struct MeshData {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    colors: Vec<[f32; 4]>,
    indices: Vec<u32>,
}

struct ChunkBuildOutput {
    coord: IVec3,
    chunk: Chunk,
    mesh_data: MeshData,
}

impl Chunk {
    fn new_terrain(coord: IVec3) -> Self {
        let blocks = vec![Block::Air; (CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE) as usize];
        let mut chunk = Self { blocks };
        let base_x = coord.x * CHUNK_SIZE;
        let base_y = coord.y * CHUNK_SIZE;
        let base_z = coord.z * CHUNK_SIZE;
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let height = height_at(base_x + x, base_z + z);
                for y in 0..CHUNK_SIZE {
                    let world_y = base_y + y;
                    if world_y > height {
                        continue;
                    }
                    let block = if world_y == height {
                        Block::Grass
                    } else {
                        Block::Dirt
                    };
                    chunk.set_block(x, y, z, block);
                }
            }
        }
        chunk
    }

    fn new_empty() -> Self {
        let blocks = vec![Block::Air; (CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE) as usize];
        Self { blocks }
    }

    fn index(x: i32, y: i32, z: i32) -> usize {
        (x + y * CHUNK_SIZE + z * CHUNK_SIZE * CHUNK_SIZE) as usize
    }

    fn in_bounds(x: i32, y: i32, z: i32) -> bool {
        (0..CHUNK_SIZE).contains(&x) && (0..CHUNK_SIZE).contains(&y) && (0..CHUNK_SIZE).contains(&z)
    }

    fn get_block(&self, x: i32, y: i32, z: i32) -> Block {
        if !Self::in_bounds(x, y, z) {
            return Block::Air;
        }
        self.blocks[Self::index(x, y, z)]
    }

    fn set_block(&mut self, x: i32, y: i32, z: i32, block: Block) {
        if !Self::in_bounds(x, y, z) {
            return;
        }
        let index = Self::index(x, y, z);
        self.blocks[index] = block;
    }
}

fn height_at(x: i32, z: i32) -> i32 {
    let fx = x as f32 * 0.08;
    let fz = z as f32 * 0.08;
    let noise = fbm_2d(fx, fz);
    let base = 4.0;
    let amp = 6.0;
    let height = (base + noise * amp).round() as i32;
    height.clamp(1, CHUNK_SIZE - 1)
}

fn fbm_2d(x: f32, z: f32) -> f32 {
    let mut value = 0.0;
    let mut amplitude = 1.0;
    let mut frequency = 1.0;
    let mut norm = 0.0;
    for _ in 0..3 {
        value += value_noise_2d(x * frequency, z * frequency) * amplitude;
        norm += amplitude;
        amplitude *= 0.5;
        frequency *= 2.0;
    }
    value / norm
}

fn value_noise_2d(x: f32, z: f32) -> f32 {
    let x0 = x.floor() as i32;
    let z0 = z.floor() as i32;
    let x1 = x0 + 1;
    let z1 = z0 + 1;
    let tx = fade(x - x0 as f32);
    let tz = fade(z - z0 as f32);

    let v00 = hash_2d(x0, z0);
    let v10 = hash_2d(x1, z0);
    let v01 = hash_2d(x0, z1);
    let v11 = hash_2d(x1, z1);

    let a = lerp(v00, v10, tx);
    let b = lerp(v01, v11, tx);
    lerp(a, b, tz)
}

fn hash_2d(x: i32, z: i32) -> f32 {
    let mut n = x as u32;
    n = n
        .wrapping_mul(374761393)
        .wrapping_add((z as u32).wrapping_mul(668265263));
    n ^= n >> 13;
    n = n.wrapping_mul(1274126177);
    let v = (n & 0x00ff_ffff) as f32 / 0x00ff_ffff as f32;
    v * 2.0 - 1.0
}

fn fade(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn build_chunk_mesh_data(chunk: &Chunk) -> MeshData {
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut uvs: Vec<[f32; 2]> = Vec::new();
    let mut colors: Vec<[f32; 4]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    for z in 0..CHUNK_SIZE {
        for y in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let block = chunk.get_block(x, y, z);
                if block == Block::Air {
                    continue;
                }
                let fx = x as f32 * BLOCK_SIZE;
                let fy = y as f32 * BLOCK_SIZE;
                let fz = z as f32 * BLOCK_SIZE;

                add_face(
                    &mut positions,
                    &mut normals,
                    &mut uvs,
                    &mut colors,
                    &mut indices,
                    [fx + BLOCK_SIZE, fy, fz],
                    [fx + BLOCK_SIZE, fy + BLOCK_SIZE, fz],
                    [fx + BLOCK_SIZE, fy + BLOCK_SIZE, fz + BLOCK_SIZE],
                    [fx + BLOCK_SIZE, fy, fz + BLOCK_SIZE],
                    [1.0, 0.0, 0.0],
                    face_color(block, [1.0, 0.0, 0.0], x, y, z),
                    chunk.get_block(x + 1, y, z) == Block::Air,
                );
                add_face(
                    &mut positions,
                    &mut normals,
                    &mut uvs,
                    &mut colors,
                    &mut indices,
                    [fx, fy, fz + BLOCK_SIZE],
                    [fx, fy + BLOCK_SIZE, fz + BLOCK_SIZE],
                    [fx, fy + BLOCK_SIZE, fz],
                    [fx, fy, fz],
                    [-1.0, 0.0, 0.0],
                    face_color(block, [-1.0, 0.0, 0.0], x, y, z),
                    chunk.get_block(x - 1, y, z) == Block::Air,
                );
                add_face(
                    &mut positions,
                    &mut normals,
                    &mut uvs,
                    &mut colors,
                    &mut indices,
                    [fx, fy + BLOCK_SIZE, fz],
                    [fx, fy + BLOCK_SIZE, fz + BLOCK_SIZE],
                    [fx + BLOCK_SIZE, fy + BLOCK_SIZE, fz + BLOCK_SIZE],
                    [fx + BLOCK_SIZE, fy + BLOCK_SIZE, fz],
                    [0.0, 1.0, 0.0],
                    face_color(block, [0.0, 1.0, 0.0], x, y, z),
                    chunk.get_block(x, y + 1, z) == Block::Air,
                );
                add_face(
                    &mut positions,
                    &mut normals,
                    &mut uvs,
                    &mut colors,
                    &mut indices,
                    [fx, fy, fz + BLOCK_SIZE],
                    [fx, fy, fz],
                    [fx + BLOCK_SIZE, fy, fz],
                    [fx + BLOCK_SIZE, fy, fz + BLOCK_SIZE],
                    [0.0, -1.0, 0.0],
                    face_color(block, [0.0, -1.0, 0.0], x, y, z),
                    chunk.get_block(x, y - 1, z) == Block::Air,
                );
                add_face(
                    &mut positions,
                    &mut normals,
                    &mut uvs,
                    &mut colors,
                    &mut indices,
                    [fx + BLOCK_SIZE, fy, fz + BLOCK_SIZE],
                    [fx + BLOCK_SIZE, fy + BLOCK_SIZE, fz + BLOCK_SIZE],
                    [fx, fy + BLOCK_SIZE, fz + BLOCK_SIZE],
                    [fx, fy, fz + BLOCK_SIZE],
                    [0.0, 0.0, 1.0],
                    face_color(block, [0.0, 0.0, 1.0], x, y, z),
                    chunk.get_block(x, y, z + 1) == Block::Air,
                );
                add_face(
                    &mut positions,
                    &mut normals,
                    &mut uvs,
                    &mut colors,
                    &mut indices,
                    [fx, fy, fz],
                    [fx, fy + BLOCK_SIZE, fz],
                    [fx + BLOCK_SIZE, fy + BLOCK_SIZE, fz],
                    [fx + BLOCK_SIZE, fy, fz],
                    [0.0, 0.0, -1.0],
                    face_color(block, [0.0, 0.0, -1.0], x, y, z),
                    chunk.get_block(x, y, z - 1) == Block::Air,
                );
            }
        }
    }

    MeshData {
        positions,
        normals,
        uvs,
        colors,
        indices,
    }
}

fn mesh_from_data(data: MeshData) -> Mesh {
    let mut mesh = Mesh::new(
        bevy::render::render_resource::PrimitiveTopology::TriangleList,
        bevy::asset::RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, data.positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, data.normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, data.uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, data.colors);
    mesh.insert_indices(bevy::mesh::Indices::U32(data.indices));
    mesh
}

fn add_face(
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    uvs: &mut Vec<[f32; 2]>,
    colors: &mut Vec<[f32; 4]>,
    indices: &mut Vec<u32>,
    v0: [f32; 3],
    v1: [f32; 3],
    v2: [f32; 3],
    v3: [f32; 3],
    normal: [f32; 3],
    color: [f32; 4],
    visible: bool,
) {
    if !visible {
        return;
    }

    let start = positions.len() as u32;
    positions.extend_from_slice(&[v0, v1, v2, v3]);
    normals.extend_from_slice(&[normal, normal, normal, normal]);
    uvs.extend_from_slice(&[[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]]);
    colors.extend_from_slice(&[color, color, color, color]);
    indices.extend_from_slice(&[start, start + 1, start + 2, start, start + 2, start + 3]);
}

fn face_color(block: Block, normal: [f32; 3], x: i32, y: i32, z: i32) -> [f32; 4] {
    let seed = (x * 7349 + y * 199 + z * 9151) as f32;
    let jitter = (seed.sin() * 0.03).clamp(-0.03, 0.03);
    match block {
        Block::Grass => {
            if normal[1] > 0.5 {
                [0.14 + jitter, 0.50 + jitter, 0.14 + jitter, 1.0]
            } else if normal[1] < -0.5 {
                [0.25 + jitter, 0.18 + jitter, 0.12 + jitter, 1.0]
            } else {
                [0.45 + jitter, 0.30 + jitter, 0.18 + jitter, 1.0]
            }
        }
        Block::Dirt => {
            if normal[1] > 0.5 {
                [0.45 + jitter, 0.32 + jitter, 0.20 + jitter, 1.0]
            } else if normal[1] < -0.5 {
                [0.25 + jitter, 0.18 + jitter, 0.12 + jitter, 1.0]
            } else {
                [0.40 + jitter, 0.28 + jitter, 0.17 + jitter, 1.0]
            }
        }
        Block::Air => [0.0, 0.0, 0.0, 0.0],
    }
}

fn face_color_preview(block: Block, normal: [f32; 3]) -> [f32; 4] {
    match block {
        Block::Grass => {
            if normal[1] > 0.5 {
                [0.14, 0.50, 0.14, 1.0]
            } else if normal[1] < -0.5 {
                [0.25, 0.18, 0.12, 1.0]
            } else {
                [0.45, 0.30, 0.18, 1.0]
            }
        }
        Block::Dirt => {
            if normal[1] > 0.5 {
                [0.45, 0.32, 0.20, 1.0]
            } else if normal[1] < -0.5 {
                [0.25, 0.18, 0.12, 1.0]
            } else {
                [0.40, 0.28, 0.17, 1.0]
            }
        }
        Block::Air => [0.0, 0.0, 0.0, 0.0],
    }
}

fn build_single_block_mesh_data(block: Block) -> MeshData {
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut uvs: Vec<[f32; 2]> = Vec::new();
    let mut colors: Vec<[f32; 4]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    let fx = 0.0;
    let fy = 0.0;
    let fz = 0.0;
    add_face(
        &mut positions,
        &mut normals,
        &mut uvs,
        &mut colors,
        &mut indices,
        [fx + BLOCK_SIZE, fy, fz],
        [fx + BLOCK_SIZE, fy + BLOCK_SIZE, fz],
        [fx + BLOCK_SIZE, fy + BLOCK_SIZE, fz + BLOCK_SIZE],
        [fx + BLOCK_SIZE, fy, fz + BLOCK_SIZE],
        [1.0, 0.0, 0.0],
        face_color_preview(block, [1.0, 0.0, 0.0]),
        true,
    );
    add_face(
        &mut positions,
        &mut normals,
        &mut uvs,
        &mut colors,
        &mut indices,
        [fx, fy, fz + BLOCK_SIZE],
        [fx, fy + BLOCK_SIZE, fz + BLOCK_SIZE],
        [fx, fy + BLOCK_SIZE, fz],
        [fx, fy, fz],
        [-1.0, 0.0, 0.0],
        face_color_preview(block, [-1.0, 0.0, 0.0]),
        true,
    );
    add_face(
        &mut positions,
        &mut normals,
        &mut uvs,
        &mut colors,
        &mut indices,
        [fx, fy + BLOCK_SIZE, fz],
        [fx, fy + BLOCK_SIZE, fz + BLOCK_SIZE],
        [fx + BLOCK_SIZE, fy + BLOCK_SIZE, fz + BLOCK_SIZE],
        [fx + BLOCK_SIZE, fy + BLOCK_SIZE, fz],
        [0.0, 1.0, 0.0],
        face_color_preview(block, [0.0, 1.0, 0.0]),
        true,
    );
    add_face(
        &mut positions,
        &mut normals,
        &mut uvs,
        &mut colors,
        &mut indices,
        [fx, fy, fz + BLOCK_SIZE],
        [fx, fy, fz],
        [fx + BLOCK_SIZE, fy, fz],
        [fx + BLOCK_SIZE, fy, fz + BLOCK_SIZE],
        [0.0, -1.0, 0.0],
        face_color_preview(block, [0.0, -1.0, 0.0]),
        true,
    );
    add_face(
        &mut positions,
        &mut normals,
        &mut uvs,
        &mut colors,
        &mut indices,
        [fx + BLOCK_SIZE, fy, fz + BLOCK_SIZE],
        [fx + BLOCK_SIZE, fy + BLOCK_SIZE, fz + BLOCK_SIZE],
        [fx, fy + BLOCK_SIZE, fz + BLOCK_SIZE],
        [fx, fy, fz + BLOCK_SIZE],
        [0.0, 0.0, 1.0],
        face_color_preview(block, [0.0, 0.0, 1.0]),
        true,
    );
    add_face(
        &mut positions,
        &mut normals,
        &mut uvs,
        &mut colors,
        &mut indices,
        [fx, fy, fz],
        [fx, fy + BLOCK_SIZE, fz],
        [fx + BLOCK_SIZE, fy + BLOCK_SIZE, fz],
        [fx + BLOCK_SIZE, fy, fz],
        [0.0, 0.0, -1.0],
        face_color_preview(block, [0.0, 0.0, -1.0]),
        true,
    );

    MeshData {
        positions,
        normals,
        uvs,
        colors,
        indices,
    }
}

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

#[derive(Component)]
struct FlyCamera {
    speed: f32,
    sensitivity: f32,
    pitch: f32,
    yaw: f32,
    target: Entity,
}

#[derive(Component)]
struct PlayerBody;

#[derive(Component)]
struct Player {
    on_ground: bool,
    jump_speed: f32,
    jump_boost_time: f32,
    half_size: Vec3,
    eye_height: f32,
    target_half_size: Vec3,
    target_eye_height: f32,
    crouching: bool,
}

#[derive(Component)]
struct PlayerController {
    speed: f32,
}

#[derive(Component, Default)]
struct Velocity(Vec3);

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
        chunks: HashMap::new(),
        material,
        center: IVec3::new(i32::MIN, i32::MIN, i32::MIN),
        needed: HashSet::new(),
        pending: VecDeque::new(),
        in_flight: HashMap::new(),
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
    let spawn_mesh = meshes.add(mesh_from_data(build_chunk_mesh_data(&spawn_chunk)));
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
        ChunkData {
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
    let player_entity = commands
        .spawn((
            PlayerBody,
            Transform::from_xyz(2.0, BLOCK_SIZE + STAND_HALF_SIZE.y, 2.0),
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
            2.0,
            BLOCK_SIZE + STAND_EYE_HEIGHT,
            2.0,
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
    let preview_mesh = meshes.add(mesh_from_data(build_single_block_mesh_data(Block::Grass)));
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

fn camera_look_system(
    mouse_motion: Res<bevy::input::mouse::AccumulatedMouseMotion>,
    mut camera_query: Query<(&mut Transform, &mut FlyCamera), Without<PlayerBody>>,
    mut body_query: Query<&mut Transform, With<PlayerBody>>,
) {
    let delta = mouse_motion.delta;
    if delta == Vec2::ZERO {
        return;
    }

    for (mut cam_transform, mut camera) in &mut camera_query {
        camera.yaw -= delta.x * camera.sensitivity;
        camera.pitch -= delta.y * camera.sensitivity;
        camera.pitch = camera.pitch.clamp(-1.54, 1.54);

        if let Ok(mut body_transform) = body_query.get_mut(camera.target) {
            body_transform.rotation = Quat::from_euler(EulerRot::YXZ, camera.yaw, 0.0, 0.0);
        }
        cam_transform.rotation = Quat::from_euler(EulerRot::YXZ, camera.yaw, camera.pitch, 0.0);
    }
}

fn camera_follow_system(
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

fn camera_move_system(
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

        let mut wish = direction;
        wish.y = 0.0;
        if wish != Vec3::ZERO {
            let mut speed = controller.speed;
            if input.pressed(KeyCode::ShiftLeft) {
                speed *= 1.5;
            }
            wish = wish.normalize() * speed;
        }
        if player.on_ground {
            velocity.0.x = wish.x;
            velocity.0.z = wish.z;
        } else if wish != Vec3::ZERO {
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

fn crouch_system(
    input: Res<ButtonInput<KeyCode>>,
    mut query: Query<(&mut Transform, &mut Player), With<PlayerBody>>,
    world: Res<WorldState>,
) {
    for (mut transform, mut player) in &mut query {
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

fn crouch_transition_system(
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
            if new_y <= old_half.y || !intersects_solid(candidate_pos, candidate_half, &world) {
                transform.translation = candidate_pos;
                player.half_size = candidate_half;
            }
        }

        player.eye_height += (player.target_eye_height - player.eye_height) * t;
    }
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

fn physics_system(
    time: Res<Time>,
    input: Res<ButtonInput<KeyCode>>,
    mut query: Query<(&mut Transform, &mut Velocity, &mut Player), With<PlayerBody>>,
    world: Res<WorldState>,
) {
    let dt = time.delta_secs();
    for (mut transform, mut velocity, mut player) in &mut query {
        if !input.pressed(KeyCode::Space) {
            player.jump_boost_time = 0.0;
        }
        if player.jump_boost_time > 0.0 {
            velocity.0.y += JUMP_BOOST_ACCEL * dt;
            player.jump_boost_time -= dt;
        }

        velocity.0.y -= GRAVITY * dt;

        let mut pos = transform.translation;
        let mut vel = velocity.0;
        player.on_ground = false;

        move_axis(Vec3::X, &mut pos, &mut vel, player.half_size, &world, dt);
        move_axis(Vec3::Z, &mut pos, &mut vel, player.half_size, &world, dt);
        move_axis(Vec3::Y, &mut pos, &mut vel, player.half_size, &world, dt);

        if vel.y == 0.0 && velocity.0.y < 0.0 {
            player.on_ground = true;
        }

        transform.translation = pos;
        velocity.0 = vel;
    }
}

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

fn intersects_solid(position: Vec3, half_size: Vec3, world: &WorldState) -> bool {
    let min = position - half_size;
    let max = position + half_size;

    let min_x = (min.x / BLOCK_SIZE).floor() as i32;
    let max_x = (max.x / BLOCK_SIZE).floor() as i32;
    let min_y = (min.y / BLOCK_SIZE).floor() as i32;
    let max_y = (max.y / BLOCK_SIZE).floor() as i32;
    let min_z = (min.z / BLOCK_SIZE).floor() as i32;
    let max_z = (max.z / BLOCK_SIZE).floor() as i32;

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

fn is_solid_world(pos: IVec3, world: &WorldState) -> bool {
    let chunk_coord = IVec3::new(
        pos.x.div_euclid(CHUNK_SIZE),
        pos.y.div_euclid(CHUNK_SIZE),
        pos.z.div_euclid(CHUNK_SIZE),
    );
    let Some(chunk) = world.chunks.get(&chunk_coord) else {
        return false;
    };
    let local = IVec3::new(
        pos.x.rem_euclid(CHUNK_SIZE),
        pos.y.rem_euclid(CHUNK_SIZE),
        pos.z.rem_euclid(CHUNK_SIZE),
    );
    if !Chunk::in_bounds(local.x, local.y, local.z) {
        return false;
    }
    matches!(
        chunk.chunk.get_block(local.x, local.y, local.z),
        Block::Grass | Block::Dirt
    )
}

fn ensure_chunk(
    commands: &mut Commands,
    world: &mut WorldState,
    meshes: &mut ResMut<Assets<Mesh>>,
    coord: IVec3,
) {
    if world.chunks.contains_key(&coord) {
        return;
    }
    let chunk = if coord.y == 0 {
        Chunk::new_terrain(coord)
    } else {
        Chunk::new_empty()
    };
    let mesh = meshes.add(mesh_from_data(build_chunk_mesh_data(&chunk)));
    let entity = commands
        .spawn((
            bevy::mesh::Mesh3d(mesh.clone()),
            bevy::pbr::MeshMaterial3d(world.material.clone()),
            Transform::from_xyz(
                coord.x as f32 * CHUNK_SIZE as f32 * BLOCK_SIZE,
                coord.y as f32 * CHUNK_SIZE as f32 * BLOCK_SIZE,
                coord.z as f32 * CHUNK_SIZE as f32 * BLOCK_SIZE,
            ),
        ))
        .id();
    world.chunks.insert(
        coord,
        ChunkData {
            chunk,
            mesh,
            entity,
        },
    );
}

fn chunk_loading_system(
    mut commands: Commands,
    mut world: ResMut<WorldState>,
    mut meshes: ResMut<Assets<Mesh>>,
    camera_query: Query<&GlobalTransform, With<bevy::camera::Camera3d>>,
) {
    let task_pool = AsyncComputeTaskPool::get();
    let Ok(camera_transform) = camera_query.single() else {
        return;
    };
    let camera_pos = camera_transform.translation();
    let center = IVec3::new(
        (camera_pos.x / (CHUNK_SIZE as f32 * BLOCK_SIZE)).floor() as i32,
        0,
        (camera_pos.z / (CHUNK_SIZE as f32 * BLOCK_SIZE)).floor() as i32,
    );
    world.center = center;

    let mut needed: HashSet<IVec3> = HashSet::new();
    for dz in -VIEW_DISTANCE..VIEW_DISTANCE {
        for dx in -VIEW_DISTANCE..VIEW_DISTANCE {
            needed.insert(center + IVec3::new(dx, 0, dz));
        }
    }
    if needed != world.needed {
        world.needed = needed;
        let needed = world.needed.clone();
        world.pending.retain(|coord| needed.contains(coord));
        world.in_flight.retain(|coord, _| needed.contains(coord));
    }

    let needed = world.needed.clone();
    for coord in needed.iter().copied() {
        if world.chunks.contains_key(&coord)
            || world.pending.contains(&coord)
            || world.in_flight.contains_key(&coord)
        {
            continue;
        }
        world.pending.push_back(coord);
    }

    let to_remove: Vec<IVec3> = world
        .chunks
        .keys()
        .copied()
        .filter(|coord| coord.y == 0 && !world.needed.contains(coord))
        .collect();
    for coord in to_remove {
        if let Some(data) = world.chunks.remove(&coord) {
            commands.entity(data.entity).despawn();
        }
    }

    let mut started = 0;
    while started < LOADS_PER_FRAME
        && world.in_flight.len() < MAX_IN_FLIGHT
        && !world.pending.is_empty()
    {
        let coord = world.pending.pop_front().unwrap();
        let task = task_pool.spawn(async move {
            let chunk = if coord.y == 0 {
                Chunk::new_terrain(coord)
            } else {
                Chunk::new_empty()
            };
            let mesh_data = build_chunk_mesh_data(&chunk);
            ChunkBuildOutput {
                coord,
                chunk,
                mesh_data,
            }
        });
        world.in_flight.insert(coord, task);
        started += 1;
    }

    let mut finished: Vec<ChunkBuildOutput> = Vec::new();
    let mut finished_coords: Vec<IVec3> = Vec::new();
    for (coord, task) in world.in_flight.iter_mut() {
        if let Some(result) = future::block_on(future::poll_once(task)) {
            finished.push(result);
            finished_coords.push(*coord);
        }
    }
    for coord in finished_coords {
        world.in_flight.remove(&coord);
    }
    for result in finished {
        if !world.needed.contains(&result.coord) {
            continue;
        }
        let mesh = meshes.add(mesh_from_data(result.mesh_data));
        let entity = commands
            .spawn((
                bevy::mesh::Mesh3d(mesh.clone()),
                bevy::pbr::MeshMaterial3d(world.material.clone()),
                Transform::from_xyz(
                    result.coord.x as f32 * CHUNK_SIZE as f32 * BLOCK_SIZE,
                    result.coord.y as f32 * CHUNK_SIZE as f32 * BLOCK_SIZE,
                    result.coord.z as f32 * CHUNK_SIZE as f32 * BLOCK_SIZE,
                ),
            ))
            .id();
        world.chunks.insert(
            result.coord,
            ChunkData {
                chunk: result.chunk,
                mesh,
                entity,
            },
        );
    }
}

fn block_interaction_system(
    mut commands: Commands,
    buttons: Res<ButtonInput<MouseButton>>,
    mut world: ResMut<WorldState>,
    mut meshes: ResMut<Assets<Mesh>>,
    time: Res<Time>,
    mut cooldown: ResMut<InteractionCooldown>,
    camera_query: Query<&GlobalTransform, With<bevy::camera::Camera3d>>,
    mut selected: ResMut<SelectedBlock>,
    mut preview_query: Query<&mut bevy::mesh::Mesh3d, With<PreviewBlock>>,
    keys: Res<ButtonInput<KeyCode>>,
    player_query: Query<(&Transform, &Player), With<PlayerBody>>,
) {
    if keys.just_pressed(KeyCode::Digit1) {
        selected.current = Block::Grass;
        update_preview_mesh(selected.current, &mut meshes, &mut preview_query);
    }
    if keys.just_pressed(KeyCode::Digit2) {
        selected.current = Block::Dirt;
        update_preview_mesh(selected.current, &mut meshes, &mut preview_query);
    }

    let Ok(camera_transform) = camera_query.single() else {
        return;
    };
    let now = time.elapsed_secs();
    let can_break = buttons.pressed(MouseButton::Left)
        && now - cooldown.last_break_time >= 0.11;
    let can_place = buttons.pressed(MouseButton::Right)
        && now - cooldown.last_place_time >= 0.11;
    if !can_break && !can_place {
        return;
    }

    let origin: Vec3 = camera_transform.translation();
    let direction = camera_transform.forward().as_vec3().normalize_or_zero();
    if direction == Vec3::ZERO {
        return;
    }

    let mut last_empty: Option<IVec3> = None;
    let mut hit: Option<IVec3> = None;
    let step = 0.1;
    let max_distance = 6.0 * BLOCK_SIZE;
    let steps = (max_distance / step) as i32;

    for i in 0..steps {
        let position = origin + direction * (i as f32 * step);
        let block_world = IVec3::new(
            (position.x / BLOCK_SIZE).floor() as i32,
            (position.y / BLOCK_SIZE).floor() as i32,
            (position.z / BLOCK_SIZE).floor() as i32,
        );
        let chunk_coord = IVec3::new(
            block_world.x.div_euclid(CHUNK_SIZE),
            block_world.y.div_euclid(CHUNK_SIZE),
            block_world.z.div_euclid(CHUNK_SIZE),
        );
        let local = IVec3::new(
            block_world.x.rem_euclid(CHUNK_SIZE),
            block_world.y.rem_euclid(CHUNK_SIZE),
            block_world.z.rem_euclid(CHUNK_SIZE),
        );
        let Some(chunk_data) = world.chunks.get(&chunk_coord) else {
            last_empty = Some(block_world);
            continue;
        };
        if !Chunk::in_bounds(local.x, local.y, local.z) {
            last_empty = Some(block_world);
            continue;
        }
        if matches!(
            chunk_data.chunk.get_block(local.x, local.y, local.z),
            Block::Grass | Block::Dirt
        ) {
            hit = Some(block_world);
            break;
        } else {
            last_empty = Some(block_world);
        }
    }

    if can_break {
        if let Some(target_world) = hit {
            let chunk_coord = IVec3::new(
                target_world.x.div_euclid(CHUNK_SIZE),
                target_world.y.div_euclid(CHUNK_SIZE),
                target_world.z.div_euclid(CHUNK_SIZE),
            );
            let local = IVec3::new(
                target_world.x.rem_euclid(CHUNK_SIZE),
                target_world.y.rem_euclid(CHUNK_SIZE),
                target_world.z.rem_euclid(CHUNK_SIZE),
            );
            let Some(chunk_data) = world.chunks.get_mut(&chunk_coord) else {
                return;
            };
            chunk_data
                .chunk
                .set_block(local.x, local.y, local.z, Block::Air);
            if let Some(mesh) = meshes.get_mut(&chunk_data.mesh) {
                *mesh = mesh_from_data(build_chunk_mesh_data(&chunk_data.chunk));
            }
            cooldown.last_break_time = now;
        } else {
            return;
        }
    }
    if can_place && let (Some(_), Some(target_world)) = (hit, last_empty)
    {
        if let Ok((player_transform, player)) = player_query.single()
            && player_intersects_block(player_transform.translation, player.half_size, target_world)
        {
            return;
        }
        let chunk_coord = IVec3::new(
            target_world.x.div_euclid(CHUNK_SIZE),
            target_world.y.div_euclid(CHUNK_SIZE),
            target_world.z.div_euclid(CHUNK_SIZE),
        );
        let local = IVec3::new(
            target_world.x.rem_euclid(CHUNK_SIZE),
            target_world.y.rem_euclid(CHUNK_SIZE),
            target_world.z.rem_euclid(CHUNK_SIZE),
        );
        ensure_chunk(&mut commands, &mut world, &mut meshes, chunk_coord);
        let Some(chunk_data) = world.chunks.get_mut(&chunk_coord) else {
            return;
        };
        chunk_data
            .chunk
            .set_block(local.x, local.y, local.z, selected.current);
        if let Some(mesh) = meshes.get_mut(&chunk_data.mesh) {
            *mesh = mesh_from_data(build_chunk_mesh_data(&chunk_data.chunk));
        }
        cooldown.last_place_time = now;
    }
}

fn player_intersects_block(player_pos: Vec3, player_half_size: Vec3, block_world: IVec3) -> bool {
    let block_min = Vec3::new(
        block_world.x as f32 * BLOCK_SIZE,
        block_world.y as f32 * BLOCK_SIZE,
        block_world.z as f32 * BLOCK_SIZE,
    );
    let block_max = block_min + Vec3::splat(BLOCK_SIZE);

    let player_min = player_pos - player_half_size;
    let player_max = player_pos + player_half_size;

    player_min.x < block_max.x
        && player_max.x > block_min.x
        && player_min.y < block_max.y
        && player_max.y > block_min.y
        && player_min.z < block_max.z
        && player_max.z > block_min.z
}

fn update_preview_mesh(
    block: Block,
    meshes: &mut ResMut<Assets<Mesh>>,
    preview_query: &mut Query<&mut bevy::mesh::Mesh3d, With<PreviewBlock>>,
) {
    let Ok(mut mesh_handle) = preview_query.single_mut() else {
        return;
    };
    let new_mesh = meshes.add(mesh_from_data(build_single_block_mesh_data(block)));
    *mesh_handle = bevy::mesh::Mesh3d(new_mesh);
}
