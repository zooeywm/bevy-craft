use bevy::prelude::*;

use crate::material_catalog::TextureId;
use crate::voxel::block_chunk::{Block, BlockKind, Facing};

/// Face classification used by block face-material lookup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FaceKind {
    /// Top (+Y) face.
    Top,
    /// Bottom (-Y) face.
    Bottom,
    /// Front (+Z) face.
    Front,
    /// Back (-Z) face.
    Back,
    /// Left/right (X axis) faces.
    SideLeftRight,
}

/// Per-face texture assignment for one block definition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FaceMaterials {
    /// Texture id used on top face.
    pub top: TextureId,
    /// Texture id used on bottom face.
    pub bottom: TextureId,
    /// Texture id used on front (+Z) face.
    pub front: TextureId,
    /// Texture id used on back (-Z) face.
    pub back: TextureId,
    /// Texture id used on left/right (X axis) faces.
    pub side_left_right: TextureId,
}

impl FaceMaterials {
    /// Return texture id for one face class.
    pub const fn texture_for_face(&self, face: FaceKind) -> TextureId {
        match face {
            FaceKind::Top => self.top,
            FaceKind::Bottom => self.bottom,
            FaceKind::Front => self.front,
            FaceKind::Back => self.back,
            FaceKind::SideLeftRight => self.side_left_right,
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
    /// Whether this block can store vertical front directions (+Y/-Y).
    pub allow_vertical_front: bool,
    /// Face material mapping for this block.
    pub materials: FaceMaterials,
}

/// Air block definition.
const AIR_DEF: BlockDef = BlockDef {
    solid: false,
    stable: false,
    interactable: false,
    allow_vertical_front: false,
    materials: FaceMaterials {
        top: TextureId::Dirt,
        bottom: TextureId::Dirt,
        front: TextureId::Dirt,
        back: TextureId::Dirt,
        side_left_right: TextureId::Dirt,
    },
};

/// Dirt block definition without grass overlay.
const DIRT_DEF: BlockDef = BlockDef {
    solid: true,
    stable: true,
    interactable: true,
    allow_vertical_front: true,
    materials: FaceMaterials {
        top: TextureId::Dirt,
        bottom: TextureId::Dirt,
        front: TextureId::Dirt,
        back: TextureId::Dirt,
        side_left_right: TextureId::Dirt,
    },
};

/// Dirt block definition with grass top/front/back/left-right textures.
const DIRT_GRASS_DEF: BlockDef = BlockDef {
    solid: true,
    stable: true,
    interactable: true,
    allow_vertical_front: false,
    materials: FaceMaterials {
        top: TextureId::GrassTop,
        bottom: TextureId::Dirt,
        front: TextureId::GrassSide,
        back: TextureId::GrassSide,
        side_left_right: TextureId::GrassSide,
    },
};

/// Sand block definition affected by gravity.
const SAND_DEF: BlockDef = BlockDef {
    solid: true,
    stable: false,
    interactable: true,
    allow_vertical_front: true,
    materials: FaceMaterials {
        top: TextureId::Sand,
        bottom: TextureId::Sand,
        front: TextureId::Sand,
        back: TextureId::Sand,
        side_left_right: TextureId::Sand,
    },
};

/// Resolve face class from world normal, using a block-local front orientation.
pub fn face_kind_from_oriented_normal(normal: IVec3, front: Facing) -> FaceKind {
    let front_normal = front.as_normal();
    if normal == front_normal {
        FaceKind::Front
    } else if normal == -front_normal {
        FaceKind::Back
    } else if normal.y > 0 {
        FaceKind::Top
    } else if normal.y < 0 {
        FaceKind::Bottom
    } else {
        FaceKind::SideLeftRight
    }
}

/// Return block definition for one block kind.
pub const fn def_for_block_kind(kind: BlockKind) -> &'static BlockDef {
    match kind {
        BlockKind::Air => &AIR_DEF,
        BlockKind::Dirt => &DIRT_DEF,
        BlockKind::DirtWithGrass => &DIRT_GRASS_DEF,
        BlockKind::Sand => &SAND_DEF,
    }
}

/// Resolve face texture id for one block face.
pub fn texture_for_face(block: Block, normal: IVec3) -> TextureId {
    let face = face_kind_from_oriented_normal(normal, block.front);
    def_for_block_kind(block.kind)
        .materials
        .texture_for_face(face)
}
