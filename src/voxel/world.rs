use bevy::prelude::*;
use bevy::tasks::AsyncComputeTaskPool;
use futures_lite::future;
use std::collections::{HashMap, HashSet, VecDeque};

use crate::BLOCK_SIZE;
use crate::player::{Player, PlayerBody};
use crate::{CHUNK_SIZE, LOADS_PER_FRAME, MAX_IN_FLIGHT, VERTICAL_CHUNK_LAYERS, VIEW_DISTANCE};

use crate::voxel::block_chunk::{Block, Chunk};
use crate::voxel::mesh::{build_chunk_mesh_data, mesh_from_data};
use crate::voxel::mesh_types::MeshData;
use crate::voxel::world_state::{ChunkBuildOutput, ChunkData, WorldState};

/// Raymarch sampling distance in world units.
const RAY_STEP: f32 = 0.1;
/// Max interaction reach measured in block lengths.
const RAY_MAX_DISTANCE_BLOCKS: f32 = 10.0;
impl WorldState {
    /// Construct an empty runtime world state with a shared material handle.
    pub fn new(material: Handle<StandardMaterial>) -> Self {
        Self {
            chunks: HashMap::new(),
            material,
            center: IVec3::new(i32::MIN, i32::MIN, i32::MIN),
            needed: HashSet::new(),
            pending: VecDeque::new(),
            in_flight: HashMap::new(),
        }
    }

    /// Spawn one chunk render entity and return its entity id.
    fn spawn_chunk_entity(
        &self,
        commands: &mut Commands,
        mesh: Handle<Mesh>,
        coord: IVec3,
    ) -> Entity {
        commands
            .spawn((
                bevy::mesh::Mesh3d(mesh),
                bevy::pbr::MeshMaterial3d(self.material.clone()),
                Transform::from_translation(Chunk::world_translation(coord)),
            ))
            .id()
    }

    /// Convert a world block coordinate into `(chunk_coord, local_coord)`.
    ///
    /// `local_coord` is normalized into `0..CHUNK_SIZE` on each axis via
    /// Euclidean division, so negative world coordinates map correctly.
    pub(crate) fn world_to_chunk_local(world_pos: IVec3) -> (IVec3, IVec3) {
        let chunk = IVec3::new(
            world_pos.x.div_euclid(CHUNK_SIZE),
            world_pos.y.div_euclid(CHUNK_SIZE),
            world_pos.z.div_euclid(CHUNK_SIZE),
        );
        let local = IVec3::new(
            world_pos.x.rem_euclid(CHUNK_SIZE),
            world_pos.y.rem_euclid(CHUNK_SIZE),
            world_pos.z.rem_euclid(CHUNK_SIZE),
        );
        (chunk, local)
    }

    /// Read a block at world-space block coordinate.
    ///
    /// Returns `None` when the containing chunk is not currently loaded.
    pub(crate) fn get_block_world(&self, world_pos: IVec3) -> Option<Block> {
        let (chunk_coord, local) = Self::world_to_chunk_local(world_pos);
        self.chunks
            .get(&chunk_coord)
            .map(|chunk| chunk.chunk.get_block(local))
    }

    /// Set block at world-space coordinate if containing chunk is loaded.
    ///
    /// Returns containing chunk coord when write succeeds.
    pub(crate) fn set_block_world_loaded(
        &mut self,
        world_pos: IVec3,
        block: Block,
    ) -> Option<IVec3> {
        let (chunk_coord, local) = Self::world_to_chunk_local(world_pos);
        let chunk_data = self.chunks.get_mut(&chunk_coord)?;
        chunk_data.chunk.set_block(local, block);
        Some(chunk_coord)
    }

    /// Ensure containing chunk exists, then set block at world-space coordinate.
    ///
    /// Returns containing chunk coord when write succeeds.
    pub(crate) fn set_block_world_ensured(
        &mut self,
        commands: &mut Commands,
        meshes: &mut ResMut<Assets<Mesh>>,
        world_pos: IVec3,
        block: Block,
    ) -> Option<IVec3> {
        let (chunk_coord, _) = Self::world_to_chunk_local(world_pos);
        self.ensure_chunk(commands, meshes, chunk_coord);
        self.set_block_world_loaded(world_pos, block)
    }

    /// Settle one falling block into voxel world at landing coordinate.
    ///
    /// Ensures target chunk exists, writes block, and returns touched chunk coord.
    pub(crate) fn settle_falling_block(
        &mut self,
        commands: &mut Commands,
        meshes: &mut ResMut<Assets<Mesh>>,
        landing_block: IVec3,
        block: Block,
    ) -> Option<IVec3> {
        self.set_block_world_ensured(commands, meshes, landing_block, block)
    }

    /// Return `true` when the world-space block coordinate is non-air.
    pub fn is_solid_at_world_pos(&self, pos: IVec3) -> bool {
        self.get_block_world(pos)
            .is_some_and(|block| block.is_solid())
    }

    /// Check whether a player-sized AABB intersects any solid block.
    pub(crate) fn intersects_solid(&self, position: Vec3, half_size: Vec3) -> bool {
        let min = position - half_size;
        let max = position + half_size;

        let min_x = (min.x / BLOCK_SIZE).floor() as i32;
        let max_x = (max.x / BLOCK_SIZE).floor() as i32;
        let min_y = (min.y / BLOCK_SIZE).floor() as i32;
        let max_y = (max.y / BLOCK_SIZE).floor() as i32;
        let min_z = (min.z / BLOCK_SIZE).floor() as i32;
        let max_z = (max.z / BLOCK_SIZE).floor() as i32;

        for z in min_z..=max_z {
            for y in min_y..=max_y {
                for x in min_x..=max_x {
                    if self.is_solid_at_world_pos(IVec3::new(x, y, z)) {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Check whether crouch edge-guard still has ground support.
    pub(crate) fn has_ground_support(&self, position: Vec3, half_size: Vec3) -> bool {
        let probe_down = BLOCK_SIZE * 0.05;
        let foot_y = position.y - half_size.y - probe_down;
        let block_y = (foot_y / BLOCK_SIZE).floor() as i32;

        // Use an inset footprint so crouch can hang slightly over an edge, similar to MC feel.
        let inset = BLOCK_SIZE * 0.2;
        let px = (half_size.x - inset).max(0.0);
        let pz = (half_size.z - inset).max(0.0);
        let probes = [
            Vec2::new(-px, -pz),
            Vec2::new(-px, pz),
            Vec2::new(px, -pz),
            Vec2::new(px, pz),
            Vec2::ZERO,
        ];

        probes.iter().any(|p| {
            let wx = position.x + p.x;
            let wz = position.z + p.y;
            let block_x = (wx / BLOCK_SIZE).floor() as i32;
            let block_z = (wz / BLOCK_SIZE).floor() as i32;
            self.is_solid_at_world_pos(IVec3::new(block_x, block_y, block_z))
        })
    }

    /// Build interaction ray from camera and run raymarch.
    pub(crate) fn raymarch_from_camera(
        &self,
        camera_transform: &GlobalTransform,
    ) -> Option<(Option<IVec3>, Option<IVec3>)> {
        let origin: Vec3 = camera_transform.translation();
        let direction = camera_transform.forward().as_vec3().normalize_or_zero();
        if direction == Vec3::ZERO {
            return None;
        }
        Some(self.raymarch_hit_and_last_empty(origin, direction))
    }

    /// Raymarch from camera and return `(first_solid_hit, last_empty_before_hit)`.
    pub(crate) fn raymarch_hit_and_last_empty(
        &self,
        origin: Vec3,
        direction: Vec3,
    ) -> (Option<IVec3>, Option<IVec3>) {
        let mut last_empty: Option<IVec3> = None;
        let mut hit: Option<IVec3> = None;
        let max_distance = RAY_MAX_DISTANCE_BLOCKS * BLOCK_SIZE;
        let steps = (max_distance / RAY_STEP) as i32;

        for i in 0..steps {
            let position = origin + direction * (i as f32 * RAY_STEP);
            let block_world = Block::world_coord_from_position(position);
            let (chunk_coord, local) = Self::world_to_chunk_local(block_world);
            let Some(chunk_data) = self.chunks.get(&chunk_coord) else {
                last_empty = Some(block_world);
                continue;
            };
            if !Chunk::in_bounds(local) {
                last_empty = Some(block_world);
                continue;
            }
            if chunk_data.chunk.get_block(local).is_solid() {
                hit = Some(block_world);
                break;
            }
            last_empty = Some(block_world);
        }

        (hit, last_empty)
    }

    /// Update `self.center` from camera position and return the new center.
    pub(crate) fn update_center_from_camera(
        &mut self,
        camera_query: &Query<&GlobalTransform, With<bevy::camera::Camera3d>>,
    ) -> Option<IVec3> {
        let center = Self::current_chunk_center(camera_query)?;
        self.center = center;
        Some(center)
    }

    /// Read current camera and compute its center chunk coordinate.
    fn current_chunk_center(
        camera_query: &Query<&GlobalTransform, With<bevy::camera::Camera3d>>,
    ) -> Option<IVec3> {
        let camera_transform = camera_query.single().ok()?;
        Some(Self::chunk_center_from_camera_pos(
            camera_transform.translation(),
        ))
    }

    /// Convert camera world-space position to horizontal center chunk coordinate.
    fn chunk_center_from_camera_pos(camera_pos: Vec3) -> IVec3 {
        IVec3::new(
            (camera_pos.x / (CHUNK_SIZE as f32 * BLOCK_SIZE)).floor() as i32,
            0,
            (camera_pos.z / (CHUNK_SIZE as f32 * BLOCK_SIZE)).floor() as i32,
        )
    }

    /// Build target chunk set inside the configured streaming window.
    pub(crate) fn build_needed_chunk_set(center: IVec3) -> HashSet<IVec3> {
        let mut needed: HashSet<IVec3> = HashSet::new();
        for dz in -VIEW_DISTANCE..VIEW_DISTANCE {
            for dx in -VIEW_DISTANCE..VIEW_DISTANCE {
                for dy in 0..VERTICAL_CHUNK_LAYERS {
                    needed.insert(center + IVec3::new(dx, dy, dz));
                }
            }
        }
        needed
    }

    /// Sync `needed` and drop pending/in-flight tasks that are no longer required.
    pub(crate) fn sync_needed_set(&mut self, needed: HashSet<IVec3>) {
        if needed == self.needed {
            return;
        }
        self.needed = needed;
        let needed = self.needed.clone();
        self.pending.retain(|coord| needed.contains(coord));
        self.in_flight.retain(|coord, _| needed.contains(coord));
    }

    /// Enqueue missing needed chunks into the build queue.
    pub(crate) fn enqueue_needed_chunks(&mut self) {
        let needed = self.needed.clone();
        for coord in needed.iter().copied() {
            if self.is_chunk_scheduled_or_loaded(coord) {
                continue;
            }
            self.pending.push_back(coord);
        }
    }

    /// Return `true` if chunk is already loaded, pending, or currently building.
    fn is_chunk_scheduled_or_loaded(&self, coord: IVec3) -> bool {
        self.chunks.contains_key(&coord)
            || self.pending.contains(&coord)
            || self.in_flight.contains_key(&coord)
    }

    /// Collect loaded chunks that are outside current needed set and should be unloaded.
    pub(crate) fn collect_unneeded_loaded_chunks(&self) -> Vec<IVec3> {
        self.chunks
            .keys()
            .copied()
            .filter(|coord| self.is_streaming_layer(*coord) && !self.needed.contains(coord))
            .collect()
    }

    /// Return `true` if chunk Y coordinate belongs to streaming vertical layers.
    fn is_streaming_layer(&self, coord: IVec3) -> bool {
        (0..VERTICAL_CHUNK_LAYERS).contains(&coord.y)
    }

    /// Spawn bounded number of async chunk build tasks for queued coordinates.
    pub(crate) fn spawn_chunk_build_tasks(&mut self, task_pool: &AsyncComputeTaskPool) {
        let mut started = 0;
        while self.can_start_chunk_build(started) {
            let coord = self.pending.pop_front().unwrap();
            let task = task_pool.spawn(async move {
                let chunk = Chunk::new_streaming(coord);
                let mesh_data = build_chunk_mesh_data(&chunk);
                ChunkBuildOutput::new(coord, chunk, mesh_data)
            });
            self.in_flight.insert(coord, task);
            started += 1;
        }
    }

    /// Return whether another chunk build task can start this frame.
    fn can_start_chunk_build(&self, started_this_frame: usize) -> bool {
        started_this_frame < LOADS_PER_FRAME
            && self.in_flight.len() < MAX_IN_FLIGHT
            && !self.pending.is_empty()
    }

    /// Poll in-flight tasks and return all finished build outputs.
    pub(crate) fn collect_finished_chunk_tasks(&mut self) -> Vec<ChunkBuildOutput> {
        let mut finished: Vec<ChunkBuildOutput> = Vec::new();
        let mut finished_coords: Vec<IVec3> = Vec::new();
        for (coord, task) in self.in_flight.iter_mut() {
            if let Some(result) = future::block_on(future::poll_once(task)) {
                finished.push(result);
                finished_coords.push(*coord);
            }
        }
        for coord in finished_coords {
            self.in_flight.remove(&coord);
        }
        finished
    }

    /// Spawn render entities and insert chunk data for finished build outputs.
    pub(crate) fn apply_finished_chunk_results(
        &mut self,
        commands: &mut Commands,
        meshes: &mut ResMut<Assets<Mesh>>,
        finished: Vec<ChunkBuildOutput>,
    ) {
        for result in finished {
            if !self.should_accept_finished_chunk(result.coord) {
                continue;
            }
            self.insert_loaded_chunk(
                commands,
                meshes,
                result.coord,
                result.chunk,
                result.mesh_data,
            );
        }
    }

    /// Return `true` if finished chunk result is still needed by current window.
    fn should_accept_finished_chunk(&self, coord: IVec3) -> bool {
        self.needed.contains(&coord)
    }

    /// Break one block at world position and rebuild touched chunk mesh.
    pub(crate) fn break_block(
        &mut self,
        meshes: &mut ResMut<Assets<Mesh>>,
        target_world: IVec3,
    ) -> bool {
        let Some(target_block) = self.get_block_world(target_world) else {
            return false;
        };
        if !target_block.is_interactable() {
            return false;
        }
        let Some(chunk_coord) = self.set_block_world_loaded(target_world, Block::air()) else {
            return false;
        };
        self.rebuild_chunk_mesh(meshes, chunk_coord);
        true
    }

    /// Place one block at world position (if not intersecting player) and rebuild mesh.
    pub(crate) fn place_block(
        &mut self,
        commands: &mut Commands,
        meshes: &mut ResMut<Assets<Mesh>>,
        player_query: &Query<(&Transform, &Player), With<PlayerBody>>,
        placement_forward: Vec3,
        target_world: IVec3,
        block: Block,
    ) -> bool {
        let mut block_to_place = block;
        if let Ok((player_transform, player)) = player_query.single() {
            if player.intersects_block(player_transform.translation, target_world) {
                return false;
            }
            // Use full 3D look direction so front can be any of 6 cardinal directions.
            block_to_place = block.with_front_from_direction(-placement_forward);
        }
        let Some(chunk_coord) =
            self.set_block_world_ensured(commands, meshes, target_world, block_to_place)
        else {
            return false;
        };
        self.rebuild_chunk_mesh(meshes, chunk_coord);
        true
    }

    /// Ensure a chunk exists at the given coordinate and spawn render entity if missing.
    pub(crate) fn ensure_chunk(
        &mut self,
        commands: &mut Commands,
        meshes: &mut ResMut<Assets<Mesh>>,
        coord: IVec3,
    ) {
        if self.chunks.contains_key(&coord) {
            return;
        }
        let chunk = Chunk::new_streaming(coord);
        let mesh = meshes.add(mesh_from_data(build_chunk_mesh_data(&chunk)));
        let entity = self.spawn_chunk_entity(commands, mesh.clone(), coord);
        self.chunks
            .insert(coord, ChunkData::new(chunk, mesh, entity));
    }

    /// Unload one chunk and despawn its render entity if present.
    pub(crate) fn unload_chunk(&mut self, commands: &mut Commands, coord: IVec3) {
        let Some(data) = self.chunks.remove(&coord) else {
            return;
        };
        commands.entity(data.entity).despawn();
    }

    /// Spawn render entity from mesh data and insert loaded chunk payload.
    pub(crate) fn insert_loaded_chunk(
        &mut self,
        commands: &mut Commands,
        meshes: &mut ResMut<Assets<Mesh>>,
        coord: IVec3,
        chunk: Chunk,
        mesh_data: MeshData,
    ) {
        let mesh = meshes.add(mesh_from_data(mesh_data));
        let entity = self.spawn_chunk_entity(commands, mesh.clone(), coord);
        self.chunks
            .insert(coord, ChunkData::new(chunk, mesh, entity));
    }

    /// Rebuild mesh for one loaded chunk if both chunk and mesh handles exist.
    pub(crate) fn rebuild_chunk_mesh(&mut self, meshes: &mut ResMut<Assets<Mesh>>, coord: IVec3) {
        let Some(chunk_data) = self.chunks.get_mut(&coord) else {
            return;
        };
        if let Some(mesh) = meshes.get_mut(&chunk_data.mesh) {
            *mesh = mesh_from_data(build_chunk_mesh_data(&chunk_data.chunk));
        }
    }

    /// Rebuild meshes for a set of touched chunk coordinates.
    pub(crate) fn rebuild_touched_chunk_meshes<I>(
        &mut self,
        meshes: &mut ResMut<Assets<Mesh>>,
        touched: I,
    ) where
        I: IntoIterator<Item = IVec3>,
    {
        for coord in touched {
            self.rebuild_chunk_mesh(meshes, coord);
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy::prelude::*;

    use super::*;

    /// Verify landing write-back updates loaded chunk voxel and reports touched chunk.
    #[test]
    fn set_block_world_loaded_writes_into_loaded_chunk() {
        let mut state = WorldState::new(Handle::<StandardMaterial>::default());
        let chunk_coord = IVec3::new(0, 0, 0);
        state.chunks.insert(
            chunk_coord,
            ChunkData::new(
                Chunk::new_empty(),
                Handle::<Mesh>::default(),
                Entity::PLACEHOLDER,
            ),
        );

        let landing_block = IVec3::new(1, 2, 3);
        let touched = state.set_block_world_loaded(landing_block, Block::dirt());
        assert_eq!(touched, Some(chunk_coord));
        assert!(
            matches!(state.get_block_world(landing_block), Some(block) if block == Block::dirt())
        );
    }
}
