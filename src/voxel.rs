use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task};
use futures_lite::future;
use std::collections::{HashMap, HashSet, VecDeque};

use crate::player::{Player, PlayerBody};
use crate::terrain::height_at;
use crate::{
    BLOCK_SIZE, CHUNK_SIZE, LOADS_PER_FRAME, MAX_IN_FLIGHT, VIEW_DISTANCE,
    VERTICAL_CHUNK_LAYERS,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Block {
    Air,
    Grass,
    Dirt,
}

pub struct Chunk {
    blocks: Vec<Block>,
}

#[derive(Resource)]
pub struct WorldState {
    pub chunks: HashMap<IVec3, ChunkData>,
    pub material: Handle<StandardMaterial>,
    pub center: IVec3,
    pub needed: HashSet<IVec3>,
    pub pending: VecDeque<IVec3>,
    pub in_flight: HashMap<IVec3, Task<ChunkBuildOutput>>,
}

pub struct ChunkData {
    pub chunk: Chunk,
    pub mesh: Handle<Mesh>,
    pub entity: Entity,
}

#[derive(Resource)]
pub struct SelectedBlock {
    pub current: Block,
}

#[derive(Resource)]
pub struct InteractionCooldown {
    pub last_break_time: f32,
    pub last_place_time: f32,
}

#[derive(Component)]
pub struct PreviewBlock;

#[derive(Component)]
pub struct Crosshair;

pub struct MeshData {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    colors: Vec<[f32; 4]>,
    indices: Vec<u32>,
}

pub struct ChunkBuildOutput {
    coord: IVec3,
    chunk: Chunk,
    mesh_data: MeshData,
}

// Atlas layout: 3 tiles in a single row (grass side, grass top, dirt).
const ATLAS_TILES_X: f32 = 3.0;
const TILE_GRASS_SIDE: u32 = 0;
const TILE_GRASS_TOP: u32 = 1;
const TILE_DIRT: u32 = 2;

impl Chunk {
    // Generate terrain blocks for a chunk based on heightmap.
    pub fn new_terrain(coord: IVec3) -> Self {
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

    // Create an empty (all air) chunk.
    pub fn new_empty() -> Self {
        let blocks = vec![Block::Air; (CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE) as usize];
        Self { blocks }
    }

    // Convert 3D local coordinates to linear index.
    fn index(x: i32, y: i32, z: i32) -> usize {
        (x + y * CHUNK_SIZE + z * CHUNK_SIZE * CHUNK_SIZE) as usize
    }

    // Check if local coordinates are inside chunk bounds.
    pub fn in_bounds(x: i32, y: i32, z: i32) -> bool {
        (0..CHUNK_SIZE).contains(&x) && (0..CHUNK_SIZE).contains(&y) && (0..CHUNK_SIZE).contains(&z)
    }

    // Read a block at local coordinates (returns Air if out of bounds).
    pub fn get_block(&self, x: i32, y: i32, z: i32) -> Block {
        if !Self::in_bounds(x, y, z) {
            return Block::Air;
        }
        self.blocks[Self::index(x, y, z)]
    }

    // Write a block at local coordinates (ignored if out of bounds).
    pub fn set_block(&mut self, x: i32, y: i32, z: i32, block: Block) {
        if !Self::in_bounds(x, y, z) {
            return;
        }
        let index = Self::index(x, y, z);
        self.blocks[index] = block;
    }
}

// Build mesh data for all visible faces in a chunk.
pub fn build_chunk_mesh_data(chunk: &Chunk) -> MeshData {
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

                // Only add a face when the neighbor is air.
                let x_pos = chunk.get_block(x + 1, y, z) == Block::Air;
                let x_neg = chunk.get_block(x - 1, y, z) == Block::Air;
                let y_pos = chunk.get_block(x, y + 1, z) == Block::Air;
                let y_neg = chunk.get_block(x, y - 1, z) == Block::Air;
                let z_pos = chunk.get_block(x, y, z + 1) == Block::Air;
                let z_neg = chunk.get_block(x, y, z - 1) == Block::Air;

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
                    face_uvs(tile_for_face(block, [1.0, 0.0, 0.0])),
                    [1.0, 0.0, 0.0],
                    face_color(block, [1.0, 0.0, 0.0], x, y, z),
                    x_pos,
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
                    face_uvs(tile_for_face(block, [-1.0, 0.0, 0.0])),
                    [-1.0, 0.0, 0.0],
                    face_color(block, [-1.0, 0.0, 0.0], x, y, z),
                    x_neg,
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
                    face_uvs(tile_for_face(block, [0.0, 1.0, 0.0])),
                    [0.0, 1.0, 0.0],
                    face_color(block, [0.0, 1.0, 0.0], x, y, z),
                    y_pos,
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
                    face_uvs(tile_for_face(block, [0.0, -1.0, 0.0])),
                    [0.0, -1.0, 0.0],
                    face_color(block, [0.0, -1.0, 0.0], x, y, z),
                    y_neg,
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
                    face_uvs(tile_for_face(block, [0.0, 0.0, 1.0])),
                    [0.0, 0.0, 1.0],
                    face_color(block, [0.0, 0.0, 1.0], x, y, z),
                    z_pos,
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
                    face_uvs(tile_for_face(block, [0.0, 0.0, -1.0])),
                    [0.0, 0.0, -1.0],
                    face_color(block, [0.0, 0.0, -1.0], x, y, z),
                    z_neg,
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

// Helper to build a Mesh directly for a chunk.
pub fn build_chunk_mesh(chunk: &Chunk) -> Mesh {
    mesh_from_data(build_chunk_mesh_data(chunk))
}

// Convert mesh data into a Bevy Mesh.
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

// Add a single quad face to mesh buffers.
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
    uv: [[f32; 2]; 4],
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
    uvs.extend_from_slice(&uv);
    colors.extend_from_slice(&[color, color, color, color]);
    indices.extend_from_slice(&[start, start + 1, start + 2, start, start + 2, start + 3]);
}

// Color blocks with small jitter for visual variation.
fn face_color(block: Block, normal: [f32; 3], x: i32, y: i32, z: i32) -> [f32; 4] {
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

// Color for the preview block without jitter.
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


// Build mesh data for a single block (used for preview).
pub fn build_single_block_mesh_data(block: Block) -> MeshData {
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
        face_uvs(tile_for_face(block, [1.0, 0.0, 0.0])),
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
        face_uvs(tile_for_face(block, [-1.0, 0.0, 0.0])),
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
        face_uvs(tile_for_face(block, [0.0, 1.0, 0.0])),
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
        face_uvs(tile_for_face(block, [0.0, -1.0, 0.0])),
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
        face_uvs(tile_for_face(block, [0.0, 0.0, 1.0])),
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
        face_uvs(tile_for_face(block, [0.0, 0.0, -1.0])),
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

// Pick atlas tile index for a block face.
fn tile_for_face(block: Block, normal: [f32; 3]) -> u32 {
    match block {
        Block::Grass => {
            if normal[1] > 0.5 {
                TILE_GRASS_TOP
            } else if normal[1] < -0.5 {
                TILE_DIRT
            } else {
                TILE_GRASS_SIDE
            }
        }
        Block::Dirt => TILE_DIRT,
        Block::Air => TILE_DIRT,
    }
}

// UVs for a tile in a 1x3 atlas.
fn face_uvs(tile: u32) -> [[f32; 2]; 4] {
    let u0 = tile as f32 / ATLAS_TILES_X;
    let u1 = (tile as f32 + 1.0) / ATLAS_TILES_X;
    [
        [u0, 0.0],
        [u0, 1.0],
        [u1, 1.0],
        [u1, 0.0],
    ]
}

// Helper to build a Mesh directly for a single block.
pub fn build_single_block_mesh(block: Block) -> Mesh {
    mesh_from_data(build_single_block_mesh_data(block))
}

// Update the preview mesh when selected block changes.
pub fn update_preview_mesh(
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

// Check if a world position contains a solid block.
pub fn is_solid_world(pos: IVec3, world: &WorldState) -> bool {
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

// Ensure a chunk exists at the given coordinate (spawn if missing).
pub fn ensure_chunk(
    commands: &mut Commands,
    world: &mut WorldState,
    meshes: &mut ResMut<Assets<Mesh>>,
    coord: IVec3,
) {
    if world.chunks.contains_key(&coord) {
        return;
    }
    let chunk = if (0..VERTICAL_CHUNK_LAYERS).contains(&coord.y) {
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

// Load/unload chunks around the player and build meshes asynchronously.
pub fn chunk_loading_system(
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
    // Keep view distance centered around the camera.
    let center = IVec3::new(
        (camera_pos.x / (CHUNK_SIZE as f32 * BLOCK_SIZE)).floor() as i32,
        0,
        (camera_pos.z / (CHUNK_SIZE as f32 * BLOCK_SIZE)).floor() as i32,
    );
    world.center = center;

    // Desired chunk set in a 3D window (x/z radius + vertical layers).
    let mut needed: HashSet<IVec3> = HashSet::new();
    for dz in -VIEW_DISTANCE..VIEW_DISTANCE {
        for dx in -VIEW_DISTANCE..VIEW_DISTANCE {
            for dy in 0..VERTICAL_CHUNK_LAYERS {
                needed.insert(center + IVec3::new(dx, dy, dz));
            }
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

    // Unload chunks that fall outside the needed set.
    let to_remove: Vec<IVec3> = world
        .chunks
        .keys()
        .copied()
        .filter(|coord| (0..VERTICAL_CHUNK_LAYERS).contains(&coord.y) && !world.needed.contains(coord))
        .collect();
    for coord in to_remove {
        if let Some(data) = world.chunks.remove(&coord) {
            commands.entity(data.entity).despawn();
        }
    }

    // Start a limited number of async chunk builds per frame.
    let mut started = 0;
    while started < LOADS_PER_FRAME
        && world.in_flight.len() < MAX_IN_FLIGHT
        && !world.pending.is_empty()
    {
        let coord = world.pending.pop_front().unwrap();
        let task = task_pool.spawn(async move {
        let chunk = if (0..VERTICAL_CHUNK_LAYERS).contains(&coord.y) {
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

    // Collect finished async tasks.
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

// Handle block breaking/placing with cooldown and preview updates.
pub fn block_interaction_system(
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
    // Rate limit repeated interactions.
    let now = time.elapsed_secs();
    let can_break = buttons.pressed(MouseButton::Left) && now - cooldown.last_break_time >= 0.2;
    let can_place = buttons.pressed(MouseButton::Right) && now - cooldown.last_place_time >= 0.2;
    if !can_break && !can_place {
        return;
    }

    let origin: Vec3 = camera_transform.translation();
    let direction = camera_transform.forward().as_vec3().normalize_or_zero();
    if direction == Vec3::ZERO {
        return;
    }

    // Track the last empty block along the ray for placement.
    let mut last_empty: Option<IVec3> = None;
    let mut hit: Option<IVec3> = None;
    let step = 0.1;
    let max_distance = 6.0 * BLOCK_SIZE;
    let steps = (max_distance / step) as i32;

    // Raymarch forward until we hit a solid block or reach max distance.
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

    // Break the first solid block hit.
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
    // Place on the last empty position before a hit.
    if can_place && let (Some(_), Some(target_world)) = (hit, last_empty) {
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

// Prevent placing blocks overlapping the player.
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
