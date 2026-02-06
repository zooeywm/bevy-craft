use bevy::prelude::*;

use crate::BLOCK_SIZE;

use crate::voxel::block_chunk::Block;

#[derive(Resource)]
/// Timer resource wrapper that controls falling-block scan frequency.
pub struct BlockFallScanTimer(
    /// Bevy timer used by the periodic falling-block scan.
    pub Timer,
);

impl BlockFallScanTimer {
    /// Build repeating scan timer with given interval seconds.
    pub fn new(interval_secs: f32) -> Self {
        Self(Timer::from_seconds(interval_secs, TimerMode::Repeating))
    }

    /// Tick scan timer and return whether this frame should run falling scan.
    pub fn should_scan(&mut self, delta: std::time::Duration) -> bool {
        self.0.tick(delta).just_finished()
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
