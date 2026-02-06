use bevy::prelude::*;

use crate::player::PreviewBlock;
use crate::player::{Player, PlayerBody};
use crate::voxel::FallingPropagationQueue;
use crate::voxel::interaction_state::{InteractionCooldown, SelectedBlock};
use crate::voxel::world_state::WorldState;

/// Return `true` only when `candidate` is one of six face-neighbors of `center`.
fn is_face_neighbor(center: IVec3, candidate: IVec3) -> bool {
    let d = candidate - center;
    d.x.abs() + d.y.abs() + d.z.abs() == 1
}

/// Handle block breaking and placing with cooldown and preview updates.
#[allow(clippy::too_many_arguments)]
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
    mut falling_queue: ResMut<FallingPropagationQueue>,
) {
    selected.apply_hotkeys(&keys, &mut meshes, &mut preview_query);

    let Ok(camera_transform) = camera_query.single() else {
        return;
    };
    // Rate limit repeated interactions.
    let can_break = cooldown.can_break(buttons.as_ref(), &time);
    let can_place = cooldown.can_place(buttons.as_ref(), &time);
    if !can_break && !can_place {
        return;
    }

    let Some((hit, last_empty)) = world.raymarch_from_camera(camera_transform) else {
        return;
    };

    // Break the first solid block hit.
    if can_break {
        if let Some(target_world) = hit {
            if !world.break_block(&mut meshes, target_world) {
                return;
            }
            falling_queue.enqueue_with_neighbors(target_world);
            cooldown.mark_break(&time);
        } else {
            return;
        }
    }

    // Place on the last empty position before a hit.
    if can_place
        && let (Some(hit_world), Some(target_world)) = (hit, last_empty)
        && is_face_neighbor(hit_world, target_world)
        && world.place_block(
            &mut commands,
            &mut meshes,
            &player_query,
            camera_transform.forward().as_vec3(),
            target_world,
            selected.current,
        )
    {
        // Re-check placed block immediately so unsupported gravity blocks fall right away.
        falling_queue.enqueue(target_world);
        cooldown.mark_place(&time);
    }
}

#[cfg(test)]
mod tests {
    use bevy::prelude::*;

    use super::*;
    use crate::voxel::WorldState;
    use crate::voxel::block_chunk::{Block, Chunk};
    use crate::voxel::world_state::ChunkData;

    /// Verify raymarch reports first solid hit and last empty block before that hit.
    #[test]
    fn raymarch_reports_hit_and_last_empty() {
        let mut world = WorldState::new(Handle::<StandardMaterial>::default());
        let mut chunk = Chunk::new_empty();
        chunk.set_block(IVec3::new(3, 0, 0), Block::dirt());
        world.chunks.insert(
            IVec3::ZERO,
            ChunkData::new(chunk, Handle::<Mesh>::default(), Entity::PLACEHOLDER),
        );

        let origin = Vec3::new(0.5, 0.5, 0.5);
        let direction = Vec3::X;
        let (hit, last_empty) = world.raymarch_hit_and_last_empty(origin, direction);

        assert_eq!(hit, Some(IVec3::new(3, 0, 0)));
        assert_eq!(last_empty, Some(IVec3::new(2, 0, 0)));
    }
}
