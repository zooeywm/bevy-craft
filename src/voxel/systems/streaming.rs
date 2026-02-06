use bevy::prelude::*;
use bevy::tasks::AsyncComputeTaskPool;

use crate::voxel::world_state::WorldState;

/// Stream chunks around camera: schedule builds, unload far chunks, apply finished results.
pub fn chunk_loading_system(
    mut commands: Commands,
    mut world: ResMut<WorldState>,
    mut meshes: ResMut<Assets<Mesh>>,
    camera_query: Query<&GlobalTransform, With<bevy::camera::Camera3d>>,
) {
    let task_pool = AsyncComputeTaskPool::get();
    let Some(center) = world.update_center_from_camera(&camera_query) else {
        return;
    };

    // Desired chunk set in a 3D window (x/z radius + vertical layers).
    let needed = WorldState::build_needed_chunk_set(center);
    world.sync_needed_set(needed);

    world.enqueue_needed_chunks();

    // Unload chunks that fall outside the needed set.
    let to_remove = world.collect_unneeded_loaded_chunks();
    for coord in to_remove {
        world.unload_chunk(&mut commands, coord);
    }

    // Start a limited number of async chunk builds per frame.
    world.spawn_chunk_build_tasks(task_pool);

    // Collect finished async tasks.
    let finished = world.collect_finished_chunk_tasks();
    world.apply_finished_chunk_results(&mut commands, &mut meshes, finished);
}
