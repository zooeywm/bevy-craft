use bevy::prelude::*;

use crate::material_catalog::TextureId;
use crate::voxel::block_chunk::Block;

/// Face classification used by block face-material lookup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FaceKind {
    /// Top (+Y) face.
    Top,
    /// Bottom (-Y) face.
    Bottom,
    /// Side (X/Z) face.
    Side,
}

/// Per-face texture assignment for one block definition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FaceMaterials {
    /// Texture id used on top face.
    pub top: TextureId,
    /// Texture id used on bottom face.
    pub bottom: TextureId,
    /// Texture id used on side faces.
    pub side: TextureId,
}

impl FaceMaterials {
    /// Return texture id for one face class.
    pub const fn texture_for_face(&self, face: FaceKind) -> TextureId {
        match face {
            FaceKind::Top => self.top,
            FaceKind::Bottom => self.bottom,
            FaceKind::Side => self.side,
        }
    }
}

/// Runtime-extensible block definition payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockDef {
    /// Whether this block occupies volume and blocks movement.
    pub solid: bool,
    /// Whether this block stays in place when unsupported.
    pub stable: bool,
    /// Whether interaction systems can directly operate on this block.
    pub interactable: bool,
    /// Face material mapping for this block.
    pub materials: FaceMaterials,
}

/// Air block definition.
const AIR_DEF: BlockDef = BlockDef {
    solid: false,
    stable: false,
    interactable: false,
    materials: FaceMaterials {
        top: TextureId::Dirt,
        bottom: TextureId::Dirt,
        side: TextureId::Dirt,
    },
};

/// Dirt block definition without grass overlay.
const DIRT_DEF: BlockDef = BlockDef {
    solid: true,
    stable: true,
    interactable: true,
    materials: FaceMaterials {
        top: TextureId::Dirt,
        bottom: TextureId::Dirt,
        side: TextureId::Dirt,
    },
};

/// Dirt block definition with grass top/side textures.
const DIRT_GRASS_DEF: BlockDef = BlockDef {
    solid: true,
    stable: true,
    interactable: true,
    materials: FaceMaterials {
        top: TextureId::GrassTop,
        bottom: TextureId::Dirt,
        side: TextureId::GrassSide,
    },
};

/// Resolve face class from block face normal.
pub const fn face_kind_from_normal(normal: IVec3) -> FaceKind {
    if normal.y > 0 {
        FaceKind::Top
    } else if normal.y < 0 {
        FaceKind::Bottom
    } else {
        FaceKind::Side
    }
}

/// Return block definition for one block value.
pub const fn def_for_block(block: Block) -> &'static BlockDef {
    match block {
        Block::Air => &AIR_DEF,
        Block::Dirt => &DIRT_DEF,
        Block::DirtWithGrass => &DIRT_GRASS_DEF,
    }
}

/// Resolve face texture id for one block face.
pub const fn texture_for_face(block: Block, normal: IVec3) -> TextureId {
    let face = face_kind_from_normal(normal);
    def_for_block(block).materials.texture_for_face(face)
}
