use bevy::prelude::*;
use bevy::tasks::Task;
use std::collections::{HashMap, HashSet, VecDeque};

use crate::voxel::block_chunk::Chunk;
use crate::voxel::mesh_types::MeshData;

/// Runtime wrapper that binds chunk voxel data to mesh/entity handles.
pub struct ChunkData {
    /// Voxel payload for this loaded chunk.
    pub chunk: Chunk,
    /// GPU mesh handle corresponding to the current chunk mesh.
    pub mesh: Handle<Mesh>,
    /// Spawned world entity that renders this chunk.
    pub entity: Entity,
}

impl ChunkData {
    /// Build runtime chunk data from voxel payload, mesh handle, and entity id.
    pub fn new(chunk: Chunk, mesh: Handle<Mesh>, entity: Entity) -> Self {
        Self { chunk, mesh, entity }
    }
}

#[derive(Resource)]
/// Global world runtime state used by chunk streaming and rendering systems.
pub struct WorldState {
    /// Loaded chunks currently present in the world.
    pub chunks: HashMap<IVec3, ChunkData>,
    /// Shared block material handle used by chunk meshes.
    pub material: Handle<StandardMaterial>,
    /// Chunk-space center around the camera/player for streaming.
    pub center: IVec3,
    /// Desired chunk set for the current streaming window.
    pub needed: HashSet<IVec3>,
    /// Chunks queued to start async generation.
    pub pending: VecDeque<IVec3>,
    /// Async chunk build tasks currently running.
    pub in_flight: HashMap<IVec3, Task<ChunkBuildOutput>>,
}

/// Result payload returned by async chunk-build tasks.
pub struct ChunkBuildOutput {
    /// Chunk coordinate produced by the async build task.
    pub(crate) coord: IVec3,
    /// Generated chunk voxel data.
    pub(crate) chunk: Chunk,
    /// Generated mesh payload for this chunk.
    pub(crate) mesh_data: MeshData,
}

impl ChunkBuildOutput {
    /// Build async chunk-build result payload.
    pub(crate) fn new(coord: IVec3, chunk: Chunk, mesh_data: MeshData) -> Self {
        Self {
            coord,
            chunk,
            mesh_data,
        }
    }
}
