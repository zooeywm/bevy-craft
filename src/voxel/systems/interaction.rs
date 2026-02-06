use bevy::prelude::*;

use crate::player::{Player, PlayerBody};
use crate::player::PreviewBlock;
use crate::voxel::interaction_state::{InteractionCooldown, SelectedBlock};
use crate::voxel::world_state::WorldState;

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
            cooldown.mark_break(&time);
        } else {
            return;
        }
    }

    // Place on the last empty position before a hit.
    if can_place
        && let (Some(_), Some(target_world)) = (hit, last_empty)
        && world.place_block(
            &mut commands,
            &mut meshes,
            &player_query,
            target_world,
            selected.current,
        )
    {
        cooldown.mark_place(&time);
    }
}

#[cfg(test)]
mod tests {
    use bevy::prelude::*;

    use super::*;
    use crate::voxel::block_chunk::{Block, Chunk};
    use crate::voxel::world_state::ChunkData;
    use crate::voxel::WorldState;

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
