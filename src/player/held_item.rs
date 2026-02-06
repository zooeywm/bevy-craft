use bevy::prelude::*;

use crate::BLOCK_SIZE;
use crate::player::FlyCamera;

/// Marker component for the single in-hand preview block entity.
#[derive(Component)]
pub struct PreviewBlock;

/// Preview follow forward offset measured in block lengths.
const PREVIEW_FOLLOW_FORWARD_BLOCKS: f32 = 2.0;
/// Preview follow right offset measured in block lengths.
const PREVIEW_FOLLOW_RIGHT_BLOCKS: f32 = 0.8;
/// Preview follow downward offset measured in block lengths.
const PREVIEW_FOLLOW_DOWN_BLOCKS: f32 = 0.6;

/// Keep the preview block transform aligned with the camera.
pub fn preview_follow_system(
    camera_query: Query<&Transform, (With<FlyCamera>, Without<PreviewBlock>)>,
    mut preview_query: Query<&mut Transform, (With<PreviewBlock>, Without<FlyCamera>)>,
) {
    let Ok(camera_transform) = camera_query.single() else {
        return;
    };
    let Ok(mut preview_transform) = preview_query.single_mut() else {
        return;
    };
    preview_transform.translation = PreviewAnchor::translation(camera_transform);
    preview_transform.rotation = camera_transform.rotation;
}

/// Preview-block anchor calculator relative to camera transform.
struct PreviewAnchor;

impl PreviewAnchor {
    /// Compute world-space preview translation from camera transform and offsets.
    fn translation(camera_transform: &Transform) -> Vec3 {
        let forward = camera_transform.forward().as_vec3();
        let right = camera_transform.right().as_vec3();
        camera_transform.translation
            + forward * (BLOCK_SIZE * PREVIEW_FOLLOW_FORWARD_BLOCKS)
            + right * (BLOCK_SIZE * PREVIEW_FOLLOW_RIGHT_BLOCKS)
            - Vec3::Y * (BLOCK_SIZE * PREVIEW_FOLLOW_DOWN_BLOCKS)
    }
}
