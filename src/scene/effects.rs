use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use crate::player::FlyCamera;

use crate::scene::SunBillboard;

/// Keep the sun billboard at a fixed direction relative to the camera.
pub fn sun_billboard_system(
    camera_query: Query<&Transform, (With<FlyCamera>, Without<SunBillboard>)>,
    mut sun_query: Query<(&SunBillboard, &mut Transform)>,
) {
    let Ok(camera_transform) = camera_query.single() else {
        return;
    };
    for (sun, mut transform) in &mut sun_query {
        sun.apply_to_transform(camera_transform, &mut transform);
    }
}

/// Factory for sun billboard visual assets.
pub(super) struct SunVisualFactory;

impl SunVisualFactory {
    /// Build a circular sun texture with a soft alpha falloff.
    pub(super) fn build_texture(size: u32) -> Image {
        let mut data = vec![0u8; (size * size * 4) as usize];
        let center = (size as f32 - 1.0) * 0.5;
        let radius = size as f32 * 0.4;
        let feather = size as f32 * 0.05;
        for y in 0..size {
            for x in 0..size {
                let dx = x as f32 - center;
                let dy = y as f32 - center;
                let dist = (dx * dx + dy * dy).sqrt();
                let t = ((radius - dist) / feather).clamp(0.0, 1.0);
                let alpha = (t * t * (3.0 - 2.0 * t) * 255.0) as u8;
                let idx = ((y * size + x) * 4) as usize;
                data[idx] = 255;
                data[idx + 1] = 245;
                data[idx + 2] = 220;
                data[idx + 3] = alpha;
            }
        }
        let size = Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        };
        let mut image = Image::new_fill(
            size,
            TextureDimension::D2,
            &[0, 0, 0, 0],
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        );
        image.data = Some(data);
        image.sampler = ImageSampler::linear();
        image
    }

    /// Build a simple quad mesh facing `+Z`.
    pub(super) fn build_quad(size: f32) -> Mesh {
        let half = size * 0.5;
        let positions = vec![
            [-half, -half, 0.0],
            [half, -half, 0.0],
            [half, half, 0.0],
            [-half, half, 0.0],
        ];
        let normals = vec![[0.0, 0.0, 1.0]; 4];
        let uvs = vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        let indices = vec![0u32, 1, 2, 0, 2, 3];
        let mut mesh = Mesh::new(
            bevy::render::render_resource::PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
        mesh.insert_indices(bevy::mesh::Indices::U32(indices));
        mesh
    }
}
