use bevy::prelude::*;
use std::collections::HashSet;

use crate::GRAVITY;

use crate::voxel::FallingPropagationQueue;
use crate::voxel::block_chunk::Block;
use crate::voxel::falling_state::FallingBlock;
use crate::voxel::mesh::build_single_block_mesh;
use crate::voxel::world_state::WorldState;

/// Max propagation nodes processed per frame to avoid long spikes.
const MAX_PROPAGATION_STEPS_PER_FRAME: usize = 256;

/// Return whether a block at `world_pos` should detach and become a falling entity.
fn should_start_falling(world: &WorldState, world_pos: IVec3, block: Block) -> bool {
    if !block.is_solid() || block.is_stable() {
        return false;
    }

    // Unstable blocks (sand-like): only check support below.
    let below = world_pos + IVec3::new(0, -1, 0);
    below.y >= 0 && !world.is_solid_at_world_pos(below)
}

/// Process falling propagation queue and spawn falling entities for unstable positions.
pub fn spawn_falling_blocks_system(
    mut commands: Commands,
    mut queue: ResMut<FallingPropagationQueue>,
    mut world: ResMut<WorldState>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let mut to_spawn: Vec<(IVec3, Block)> = Vec::new();
    for _ in 0..MAX_PROPAGATION_STEPS_PER_FRAME {
        let Some(world_pos) = queue.pop() else {
            break;
        };
        let Some(block) = world.get_block_world(world_pos) else {
            continue;
        };
        if should_start_falling(&world, world_pos, block) {
            to_spawn.push((world_pos, block));
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

        // Block removal may destabilize surrounding neighbors.
        queue.enqueue_with_neighbors(world_pos);
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
            if let Some(chunk_coord) =
                world.settle_falling_block(&mut commands, &mut meshes, landing_block, falling.block)
            {
                touched.insert(chunk_coord);
            }
            commands.entity(entity).despawn();
            continue;
        }

        transform.translation = next;
    }

    world.rebuild_touched_chunk_meshes(&mut meshes, touched);
}
