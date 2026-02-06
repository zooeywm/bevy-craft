use bevy::prelude::*;

mod effects;
mod setup;

pub use effects::sun_billboard_system;
pub use setup::{setup_cursor, setup_scene};

/// Billboard marker and parameters for the rendered sun quad.
#[derive(Component)]
pub(crate) struct SunBillboard {
    /// Normalized direction from camera toward the sun billboard.
    pub(crate) direction: Vec3,
    /// Distance from camera at which the billboard is rendered.
    pub(crate) distance: f32,
}

impl SunBillboard {
    /// Build billboard parameters from a world-space sun position and display distance.
    pub(crate) fn from_world_position(sun_position: Vec3, distance: f32) -> Self {
        Self {
            direction: sun_position.normalize_or_zero(),
            distance,
        }
    }

    /// Apply billboard translation/orientation so the quad always faces the camera.
    pub(crate) fn apply_to_transform(
        &self,
        camera_transform: &Transform,
        transform: &mut Transform,
    ) {
        transform.translation = camera_transform.translation + self.direction * self.distance;
        transform.look_at(camera_transform.translation, Vec3::Y);
    }
}
