use bevy::prelude::*;

use crate::player::components::{FlyCamera, Player, PlayerBody};

/// Update camera rotation from mouse motion and rotate player-body yaw.
pub fn camera_look_system(
    mouse_motion: Res<bevy::input::mouse::AccumulatedMouseMotion>,
    mut camera_query: Query<(&mut Transform, &mut FlyCamera), Without<PlayerBody>>,
    mut body_query: Query<&mut Transform, With<PlayerBody>>,
) {
    for (mut cam_transform, mut camera) in &mut camera_query {
        camera.apply_mouse_look(mouse_motion.delta);

        if let Ok(mut body_transform) = body_query.get_mut(camera.target) {
            body_transform.rotation = camera.body_rotation();
        }
        cam_transform.rotation = camera.camera_rotation();
    }
}

/// Keep the camera positioned at the player's eye height.
#[allow(clippy::type_complexity)]
pub fn camera_follow_system(
    mut camera_query: Query<(&mut Transform, &FlyCamera), Without<PlayerBody>>,
    body_query: Query<(&Transform, &Player), (With<PlayerBody>, Without<FlyCamera>)>,
) {
    for (mut cam_transform, camera) in &mut camera_query {
        if let Ok((body_transform, player)) = body_query.get(camera.target) {
            cam_transform.translation = camera.follow_translation(body_transform.translation, player);
        }
    }
}
