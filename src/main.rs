use bevy::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_systems(Startup, (setup_scene, setup_cursor))
        .add_systems(Update, (camera_look_system, camera_move_system))
        .run();
}

#[derive(Component)]
struct FlyCamera {
    speed: f32,
    sensitivity: f32,
    pitch: f32,
    yaw: f32,
}

fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Ground
    commands.spawn((
        bevy::mesh::Mesh3d(meshes.add(Mesh::from(bevy::math::primitives::Cuboid::new(
            20.0, 1.0, 20.0,
        )))),
        bevy::pbr::MeshMaterial3d(materials.add(bevy::pbr::StandardMaterial {
            base_color: Color::srgb(0.3, 0.7, 0.3),
            ..default()
        })),
        Transform::from_xyz(0.0, -0.5, 0.0),
    ));

    // Light
    commands.spawn((
        bevy::light::DirectionalLight {
            illuminance: 20_000.0,
            ..default()
        },
        Transform::from_xyz(5.0, 8.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // Camera
    commands
        .spawn((
            bevy::camera::Camera3d::default(),
            Transform::from_xyz(6.0, 6.0, 10.0),
        ))
        .insert(FlyCamera {
            speed: 10.0,
            sensitivity: 0.002,
            pitch: -0.35,
            yaw: -2.3,
        });
}

fn setup_cursor(
    mut windows: Query<&mut bevy::window::CursorOptions, With<bevy::window::PrimaryWindow>>,
) {
    let Ok(mut cursor_options) = windows.single_mut() else {
        return;
    };
    cursor_options.grab_mode = bevy::window::CursorGrabMode::Locked;
    cursor_options.visible = false;
}

fn camera_look_system(
    mouse_motion: Res<bevy::input::mouse::AccumulatedMouseMotion>,
    mut query: Query<(&mut Transform, &mut FlyCamera)>,
) {
    let delta = mouse_motion.delta;
    if delta == Vec2::ZERO {
        return;
    }

    for (mut transform, mut camera) in &mut query {
        camera.yaw -= delta.x * camera.sensitivity;
        camera.pitch -= delta.y * camera.sensitivity;
        camera.pitch = camera.pitch.clamp(-1.54, 1.54);
        transform.rotation = Quat::from_euler(EulerRot::YXZ, camera.yaw, camera.pitch, 0.0);
    }
}

fn camera_move_system(
    time: Res<Time>,
    input: Res<ButtonInput<KeyCode>>,
    mut query: Query<(&mut Transform, &FlyCamera)>,
) {
    for (mut transform, camera) in &mut query {
        let mut direction = Vec3::ZERO;
        if input.pressed(KeyCode::KeyW) {
            direction += transform.forward().as_vec3();
        }
        if input.pressed(KeyCode::KeyS) {
            direction -= transform.forward().as_vec3();
        }
        if input.pressed(KeyCode::KeyA) {
            direction -= transform.right().as_vec3();
        }
        if input.pressed(KeyCode::KeyD) {
            direction += transform.right().as_vec3();
        }
        if input.pressed(KeyCode::Space) {
            direction += Vec3::Y;
        }
        if input.pressed(KeyCode::ShiftLeft) {
            direction -= Vec3::Y;
        }

        if direction != Vec3::ZERO {
            let delta = direction.normalize() * camera.speed * time.delta_secs();
            transform.translation += delta;
        }
    }
}
