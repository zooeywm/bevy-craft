use bevy::prelude::*;

use crate::material_catalog::{atlas_tile_index, atlas_tiles_x, needs_v_flip};
use crate::voxel::block_chunk::Block;
use crate::voxel::mesh_types::FaceUv;

/// Atlas helper for block-face tile selection and UV generation.
pub(super) struct BlockAtlas;

impl BlockAtlas {
    /// Resolve final face UVs for a block face.
    ///
    /// Some tiles use flipped V to match source texture orientation.
    pub(super) fn face_uvs_for_face(block: Block, normal: IVec3) -> FaceUv {
        let texture = block.texture_for_face(normal);
        let tile = atlas_tile_index(texture);
        if needs_v_flip(texture) {
            Self::face_uvs_flipped_v(tile)
        } else {
            Self::face_uvs(tile)
        }
    }

    /// Build UVs for one tile in the 1x3 atlas.
    fn face_uvs(tile: u32) -> FaceUv {
        let atlas_tiles_x = atlas_tiles_x();
        let u0 = tile as f32 / atlas_tiles_x;
        let u1 = (tile as f32 + 1.0) / atlas_tiles_x;
        FaceUv([
            Vec2::new(u0, 0.0),
            Vec2::new(u0, 1.0),
            Vec2::new(u1, 1.0),
            Vec2::new(u1, 0.0),
        ])
    }

    /// Build UVs for one tile with V flipped (used by grass-side orientation fix).
    fn face_uvs_flipped_v(tile: u32) -> FaceUv {
        let atlas_tiles_x = atlas_tiles_x();
        let u0 = tile as f32 / atlas_tiles_x;
        let u1 = (tile as f32 + 1.0) / atlas_tiles_x;
        FaceUv([
            Vec2::new(u0, 1.0),
            Vec2::new(u0, 0.0),
            Vec2::new(u1, 0.0),
            Vec2::new(u1, 1.0),
        ])
    }
}
