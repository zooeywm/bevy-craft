use bevy::prelude::*;
use std::collections::HashSet;

use crate::{CHUNK_SIZE, GRAVITY};

use crate::voxel::block_chunk::Block;
use crate::voxel::falling_state::{BlockFallScanTimer, FallingBlock};
use crate::voxel::mesh::build_single_block_mesh;
use crate::voxel::world_state::WorldState;

/// Scan loaded chunks for unstable blocks and spawn falling entities.
pub fn spawn_falling_blocks_system(
    mut commands: Commands,
    time: Res<Time>,
    mut timer: ResMut<BlockFallScanTimer>,
    mut world: ResMut<WorldState>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    if !timer.should_scan(time.delta()) {
        return;
    }

    let mut to_spawn: Vec<(IVec3, Block)> = Vec::new();
    for (chunk_coord, chunk_data) in world.chunks.iter() {
        let base = *chunk_coord * CHUNK_SIZE;
        for z in 0..CHUNK_SIZE {
            for y in 0..CHUNK_SIZE {
                for x in 0..CHUNK_SIZE {
                    let block = chunk_data.chunk.get_block(IVec3::new(x, y, z));
                    if !block.is_falling_candidate() {
                        continue;
                    }
                    let world_pos = base + IVec3::new(x, y, z);
                    let below = world_pos + IVec3::new(0, -1, 0);
                    if below.y < 0 {
                        continue;
                    }
                    if world.is_air_at_world_pos(below) {
                        to_spawn.push((world_pos, block));
                    }
                }
            }
        }
    }

    if to_spawn.is_empty() {
        return;
    }

    let mut touched: HashSet<IVec3> = HashSet::new();
    for (world_pos, block) in to_spawn {
        let (chunk_coord, local) = WorldState::world_to_chunk_local(world_pos);
        let Some(chunk_data) = world.chunks.get_mut(&chunk_coord) else {
            continue;
        };
        chunk_data.chunk.set_block(local, Block::air());
        touched.insert(chunk_coord);

        let mesh = meshes.add(build_single_block_mesh(block));
        let translation = Block::world_translation(world_pos);
        commands.spawn((
            bevy::mesh::Mesh3d(mesh),
            bevy::pbr::MeshMaterial3d(world.material.clone()),
            Transform::from_translation(translation),
            FallingBlock::new(block),
            Name::new("FallingBlock"),
        ));
    }

    world.rebuild_touched_chunk_meshes(&mut meshes, touched);
}

/// Simulate falling-block entities and settle them into chunk voxels on landing.
pub fn update_falling_blocks_system(
    mut commands: Commands,
    time: Res<Time>,
    mut world: ResMut<WorldState>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut query: Query<(Entity, &mut Transform, &mut FallingBlock)>,
) {
    let dt = time.delta_secs();
    let mut touched: HashSet<IVec3> = HashSet::new();

    for (entity, mut transform, mut falling) in &mut query {
        let mut next = transform.translation;
        next.y += falling.integrate_vertical(dt, GRAVITY);

        let (below, landing_block) = FallingBlock::landing_probe(next);

        if below.y >= 0 && world.is_solid_at_world_pos(below) {
            if let Some(chunk_coord) = world.settle_falling_block(
                &mut commands,
                &mut meshes,
                landing_block,
                falling.block,
            ) {
                touched.insert(chunk_coord);
            }
            commands.entity(entity).despawn();
            continue;
        }

        transform.translation = next;
    }

    world.rebuild_touched_chunk_meshes(&mut meshes, touched);
}
