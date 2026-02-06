use bevy::prelude::*;

use crate::player::components::{Player, PlayerBody, PlayerController, Velocity};

/// Process movement input and update desired player velocity.
pub fn camera_move_system(
    input: Res<ButtonInput<KeyCode>>,
    mut query: Query<(&Transform, &PlayerController, &mut Velocity, &mut Player), With<PlayerBody>>,
) {
    for (transform, controller, mut velocity, mut player) in &mut query {
        let direction = controller.desired_direction(&input, transform, player.flying);

        // Flying mode: full 3D movement, no gravity or jump boost.
        if player.flying {
            let wish = controller.wish_velocity(
                direction,
                true,
                input.pressed(KeyCode::ShiftLeft),
            );
            velocity.0 = wish;
            player.jump_boost_time = 0.0;
        } else {
            let wish = controller.wish_velocity(
                direction,
                false,
                input.pressed(KeyCode::ShiftLeft),
            );
            player.apply_horizontal_movement(&mut velocity.0, wish);

            if input.just_pressed(KeyCode::Space) && player.on_ground {
                player.try_start_jump(&mut velocity.0);
            }
        }
    }
}

/// Toggle fly mode.
pub fn toggle_fly_system(
    input: Res<ButtonInput<KeyCode>>,
    mut query: Query<&mut Player, With<PlayerBody>>,
) {
    for mut player in &mut query {
        player.handle_fly_toggle_hotkey(&input);
    }
}
