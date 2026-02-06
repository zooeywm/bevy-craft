use bevy::prelude::*;
use std::collections::{HashSet, VecDeque};

use crate::BLOCK_SIZE;

use crate::voxel::block_chunk::Block;

#[derive(Resource, Default)]
/// Queue of world positions that need falling-support re-evaluation.
pub struct FallingPropagationQueue {
    /// Pending positions to process.
    pending: VecDeque<IVec3>,
    /// Set used to deduplicate pending positions.
    scheduled: HashSet<IVec3>,
}

impl FallingPropagationQueue {
    /// Enqueue one world block position for propagation.
    pub fn enqueue(&mut self, pos: IVec3) {
        if self.scheduled.insert(pos) {
            self.pending.push_back(pos);
        }
    }

    /// Enqueue one world position and its affected neighbors for support propagation.
    ///
    /// We intentionally skip downward propagation: support loss/gain impacts the
    /// changed block itself, its horizontal neighbors, and blocks above.
    pub fn enqueue_with_neighbors(&mut self, pos: IVec3) {
        self.enqueue(pos);
        self.enqueue(pos + IVec3::new(1, 0, 0));
        self.enqueue(pos + IVec3::new(-1, 0, 0));
        self.enqueue(pos + IVec3::new(0, 1, 0));
        self.enqueue(pos + IVec3::new(0, 0, 1));
        self.enqueue(pos + IVec3::new(0, 0, -1));
    }

    /// Pop one pending position from the queue.
    pub fn pop(&mut self) -> Option<IVec3> {
        let pos = self.pending.pop_front()?;
        self.scheduled.remove(&pos);
        Some(pos)
    }
}

#[derive(Component)]
/// Runtime state for a block currently simulated as a falling entity.
pub struct FallingBlock {
    /// Block state carried by the falling entity.
    pub(crate) block: Block,
    /// Current vertical velocity in world units per second.
    pub(crate) velocity_y: f32,
}

impl FallingBlock {
    /// Build falling-block runtime state with default initial velocity.
    pub(crate) fn new(block: Block) -> Self {
        Self {
            block,
            velocity_y: 0.0,
        }
    }

    /// Integrate vertical velocity by gravity and return the frame displacement on Y.
    pub(crate) fn integrate_vertical(&mut self, dt: f32, gravity: f32) -> f32 {
        self.velocity_y -= gravity * dt;
        self.velocity_y * dt
    }

    /// Compute `(below_block, landing_block)` from next world translation.
    pub(crate) fn landing_probe(next_translation: Vec3) -> (IVec3, IVec3) {
        let half = BLOCK_SIZE * 0.5;
        let center_x = next_translation.x + half;
        let center_z = next_translation.z + half;
        let world_x = (center_x / BLOCK_SIZE).floor() as i32;
        let world_z = (center_z / BLOCK_SIZE).floor() as i32;
        let below_y = (next_translation.y / BLOCK_SIZE).floor() as i32 - 1;
        let below = IVec3::new(world_x, below_y, world_z);
        let landing = IVec3::new(world_x, below_y + 1, world_z);
        (below, landing)
    }
}
