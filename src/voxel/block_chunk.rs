use bevy::prelude::*;

use crate::material_catalog::TextureId;
use crate::terrain::TerrainNoise;
use crate::voxel::block_defs::def_for_block_kind;
use crate::voxel::block_defs::texture_for_face;
use crate::{BLOCK_SIZE, CHUNK_SIZE, VERTICAL_CHUNK_LAYERS};

/// 3D front orientation stored on direction-sensitive blocks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Facing {
    /// Front points to world +X.
    PosX,
    /// Front points to world -X.
    NegX,
    /// Front points to world +Y.
    PosY,
    /// Front points to world -Y.
    NegY,
    /// Front points to world +Z.
    PosZ,
    /// Front points to world -Z.
    NegZ,
}

impl Facing {
    /// Resolve nearest cardinal 3D facing from a world-space direction vector.
    pub fn from_direction(direction: Vec3) -> Self {
        if direction.length_squared() == 0.0 {
            Self::PosZ
        } else if direction.x.abs() >= direction.y.abs() && direction.x.abs() >= direction.z.abs() {
            if direction.x >= 0.0 {
                Self::PosX
            } else {
                Self::NegX
            }
        } else if direction.y.abs() >= direction.x.abs() && direction.y.abs() >= direction.z.abs() {
            if direction.y >= 0.0 {
                Self::PosY
            } else {
                Self::NegY
            }
        } else if direction.z >= 0.0 {
            Self::PosZ
        } else {
            Self::NegZ
        }
    }

    /// Resolve nearest cardinal horizontal facing (never returns +Y/-Y).
    pub fn from_horizontal_direction(direction: Vec3) -> Self {
        if direction.x.abs() >= direction.z.abs() {
            if direction.x >= 0.0 {
                Self::PosX
            } else {
                Self::NegX
            }
        } else if direction.z >= 0.0 {
            Self::PosZ
        } else {
            Self::NegZ
        }
    }

    /// Return this facing as a world-space unit normal.
    pub const fn as_normal(self) -> IVec3 {
        match self {
            Self::PosX => IVec3::X,
            Self::NegX => IVec3::NEG_X,
            Self::PosY => IVec3::Y,
            Self::NegY => IVec3::NEG_Y,
            Self::PosZ => IVec3::Z,
            Self::NegZ => IVec3::NEG_Z,
        }
    }
}

/// Semantic block kind used for behavior/material lookup.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BlockKind {
    /// Empty cell with no geometry or collision.
    Air,
    /// Plain dirt block.
    Dirt,
    /// Dirt block with grass textures on top/sides.
    DirtWithGrass,
    /// Sand block affected by gravity when unsupported.
    Sand,
}

/// Voxel block state stored in chunk cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Block {
    /// Semantic kind of this block.
    pub kind: BlockKind,
    /// Local "front" direction used to resolve front/back face materials.
    pub front: Facing,
}

impl Block {
    /// Construct an air block.
    pub fn air() -> Self {
        Self {
            kind: BlockKind::Air,
            front: Facing::PosZ,
        }
    }

    /// Construct a plain dirt block without grass overlay.
    pub fn dirt() -> Self {
        Self {
            kind: BlockKind::Dirt,
            front: Facing::PosZ,
        }
    }

    /// Construct a plain dirt block with an explicit local front.
    pub fn dirt_facing(front: Facing) -> Self {
        Self {
            kind: BlockKind::Dirt,
            front,
        }
    }

    /// Construct a dirt block with grass overlay enabled.
    pub fn dirt_with_grass() -> Self {
        Self {
            kind: BlockKind::DirtWithGrass,
            front: Facing::PosZ,
        }
    }

    /// Construct a dirt-with-grass block with an explicit local front.
    pub fn dirt_with_grass_facing(front: Facing) -> Self {
        Self {
            kind: BlockKind::DirtWithGrass,
            front,
        }
    }

    /// Construct a sand block.
    pub fn sand() -> Self {
        Self {
            kind: BlockKind::Sand,
            front: Facing::PosZ,
        }
    }

    /// Construct a sand block with an explicit local front.
    pub fn sand_facing(front: Facing) -> Self {
        Self {
            kind: BlockKind::Sand,
            front,
        }
    }

    /// Return `true` if this block is air.
    pub fn is_air(&self) -> bool {
        matches!(self.kind, BlockKind::Air)
    }

    /// Return `true` if this block should not fall under gravity rules.
    pub fn is_stable(&self) -> bool {
        def_for_block_kind(self.kind).stable
    }

    /// Return `true` if interaction systems can operate on this block.
    pub fn is_interactable(&self) -> bool {
        def_for_block_kind(self.kind).interactable
    }

    /// Return `true` if this block occupies space (non-air).
    pub fn is_solid(&self) -> bool {
        def_for_block_kind(self.kind).solid
    }

    /// Resolve atlas texture id for one face normal.
    pub fn texture_for_face(&self, normal: IVec3) -> TextureId {
        texture_for_face(*self, normal)
    }

    /// Return a copy of this block whose front matches the given world-space direction.
    pub fn with_front_from_direction(self, direction: Vec3) -> Self {
        let front = if def_for_block_kind(self.kind).allow_vertical_front {
            Facing::from_direction(direction)
        } else {
            Facing::from_horizontal_direction(direction)
        };
        match self.kind {
            BlockKind::Dirt => Self::dirt_facing(front),
            BlockKind::DirtWithGrass => Self::dirt_with_grass_facing(front),
            BlockKind::Sand => Self::sand_facing(front),
            BlockKind::Air => self,
        }
    }

    /// Convert a world-space block coordinate to its minimum world-space corner.
    pub fn world_translation(block_coord: IVec3) -> Vec3 {
        Vec3::new(
            block_coord.x as f32 * BLOCK_SIZE,
            block_coord.y as f32 * BLOCK_SIZE,
            block_coord.z as f32 * BLOCK_SIZE,
        )
    }

    /// Convert world-space position to integer world-space block coordinate.
    pub fn world_coord_from_position(position: Vec3) -> IVec3 {
        IVec3::new(
            (position.x / BLOCK_SIZE).floor() as i32,
            (position.y / BLOCK_SIZE).floor() as i32,
            (position.z / BLOCK_SIZE).floor() as i32,
        )
    }
}

/// Pure voxel storage for one chunk (no ECS/render handles).
pub struct Chunk {
    /// Flat storage for CHUNK_SIZE^3 blocks in local chunk coordinates.
    blocks: Vec<Block>,
}

impl Chunk {
    /// Convert chunk grid coordinate to world-space translation (chunk origin).
    pub fn world_translation(coord: IVec3) -> Vec3 {
        Vec3::new(
            coord.x as f32 * CHUNK_SIZE as f32 * BLOCK_SIZE,
            coord.y as f32 * CHUNK_SIZE as f32 * BLOCK_SIZE,
            coord.z as f32 * CHUNK_SIZE as f32 * BLOCK_SIZE,
        )
    }

    /// Build terrain chunk for valid vertical layers, otherwise return an empty chunk.
    pub fn new_streaming(coord: IVec3) -> Self {
        if (0..VERTICAL_CHUNK_LAYERS).contains(&coord.y) {
            Self::new_terrain(coord)
        } else {
            Self::new_empty()
        }
    }

    /// Generate terrain blocks for one chunk from the heightmap function.
    pub fn new_terrain(coord: IVec3) -> Self {
        let mut chunk = Self::new_empty();
        let base_x = coord.x * CHUNK_SIZE;
        let base_y = coord.y * CHUNK_SIZE;
        let base_z = coord.z * CHUNK_SIZE;
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let height = TerrainNoise::height_at(base_x + x, base_z + z);
                for y in 0..CHUNK_SIZE {
                    let world_y = base_y + y;
                    if world_y > height {
                        continue;
                    }
                    let block = if world_y == height {
                        Block::dirt_with_grass()
                    } else {
                        Block::dirt()
                    };
                    chunk.set_block(IVec3::new(x, y, z), block);
                }
            }
        }
        chunk
    }

    /// Create an empty chunk filled with air blocks.
    pub fn new_empty() -> Self {
        let blocks = vec![Block::air(); (CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE) as usize];
        Self { blocks }
    }

    /// Convert local `(x, y, z)` coordinates to flat storage index.
    fn index(local: IVec3) -> usize {
        (local.x + local.y * CHUNK_SIZE + local.z * CHUNK_SIZE * CHUNK_SIZE) as usize
    }

    /// Return `true` if local coordinates are inside chunk bounds.
    pub fn in_bounds(local: IVec3) -> bool {
        (0..CHUNK_SIZE).contains(&local.x)
            && (0..CHUNK_SIZE).contains(&local.y)
            && (0..CHUNK_SIZE).contains(&local.z)
    }

    /// Read a block at local coordinates (returns air when out of bounds).
    pub fn get_block(&self, local: IVec3) -> Block {
        if !Self::in_bounds(local) {
            return Block::air();
        }
        self.blocks[Self::index(local)]
    }

    /// Write a block at local coordinates (ignores out-of-bounds writes).
    pub fn set_block(&mut self, local: IVec3, block: Block) {
        if !Self::in_bounds(local) {
            return;
        }
        let index = Self::index(local);
        self.blocks[index] = block;
    }
}

#[cfg(test)]
mod tests {
    use super::Block;

    /// Verify stable/falling classification for all current block variants.
    #[test]
    fn block_stability_and_falling_rules() {
        let air = Block::air();
        assert!(air.is_air());
        assert!(!air.is_solid());
        assert!(!air.is_stable());

        let dirt = Block::dirt();
        assert!(!dirt.is_air());
        assert!(dirt.is_solid());
        assert!(dirt.is_stable());

        let grass_dirt = Block::dirt_with_grass();
        assert!(grass_dirt.is_solid());
        assert!(grass_dirt.is_stable());

        let sand = Block::sand();
        assert!(sand.is_solid());
        assert!(!sand.is_stable());
    }
}
