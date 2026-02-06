use crate::CHUNK_SIZE;

/// Terrain noise generator with mountain/plains shaping constants.
pub struct TerrainNoise;

impl TerrainNoise {
    /// Base ground level for the heightmap.
    const BASE_HEIGHT: f32 = 4.0;
    /// Small amplitude for plains to keep them flat.
    const PLAIN_AMPLITUDE: f32 = 0.9;
    /// Large amplitude for mountains to make them tall.
    const MOUNTAIN_AMPLITUDE: f32 = 100.0;
    /// Weight of mountain regions (higher means denser mountains).
    const MOUNTAIN_WEIGHT: f32 = 0.4;
    /// How flat mountain tops become (0.0 none, 1.0 strong flattening).
    const MOUNTAIN_PLATEAU_WEIGHT: f32 = 0.55;
    /// Threshold for starting plateau flattening in mask space.
    const MOUNTAIN_PLATEAU_START: f32 = 0.7;
    /// Controls slope shaping (`>1` steeper, `<1` smoother).
    const SLOPE_STEEPNESS: f32 = 0.20;
    /// Noise scale for general terrain undulation.
    const TERRAIN_SCALE: f32 = 0.06;
    /// Noise scale for mountain mask distribution.
    const MOUNTAIN_SCALE: f32 = 0.18;

    /// Compute terrain height at `(x, z)` using layered value-noise.
    pub fn height_at(x: i32, z: i32) -> i32 {
        let fx = x as f32 * Self::TERRAIN_SCALE;
        let fz = z as f32 * Self::TERRAIN_SCALE;

        let noise = Self::fbm_2d(fx, fz);
        let mask = (Self::fbm_2d(fx * Self::MOUNTAIN_SCALE, fz * Self::MOUNTAIN_SCALE) + 1.0) * 0.5;
        let mountain_mask = mask.powf(2.0);
        let mut amp = Self::lerp(
            Self::PLAIN_AMPLITUDE,
            Self::MOUNTAIN_AMPLITUDE,
            mountain_mask * Self::MOUNTAIN_WEIGHT,
        );
        let plateau = Self::smoothstep(Self::MOUNTAIN_PLATEAU_START, 1.0, mountain_mask);
        amp *= Self::lerp(1.0, 1.0 - Self::MOUNTAIN_PLATEAU_WEIGHT, plateau);
        let shaped = noise.signum() * noise.abs().powf(Self::SLOPE_STEEPNESS);
        let height = (Self::BASE_HEIGHT + shaped * amp).round() as i32;
        height.clamp(1, CHUNK_SIZE * 2 - 1)
    }

    /// Compute 2D fractal Brownian motion from value-noise octaves.
    fn fbm_2d(x: f32, z: f32) -> f32 {
        let mut value = 0.0;
        let mut amplitude = 1.0;
        let mut frequency = 1.0;
        let mut norm = 0.0;
        for _ in 0..3 {
            value += Self::value_noise_2d(x * frequency, z * frequency) * amplitude;
            norm += amplitude;
            amplitude *= 0.5;
            frequency *= 2.0;
        }
        value / norm
    }

    /// Sample smooth 2D value noise with bilinear interpolation.
    fn value_noise_2d(x: f32, z: f32) -> f32 {
        let x0 = x.floor() as i32;
        let z0 = z.floor() as i32;
        let x1 = x0 + 1;
        let z1 = z0 + 1;
        let tx = Self::fade(x - x0 as f32);
        let tz = Self::fade(z - z0 as f32);

        let v00 = Self::hash_2d(x0, z0);
        let v10 = Self::hash_2d(x1, z0);
        let v01 = Self::hash_2d(x0, z1);
        let v11 = Self::hash_2d(x1, z1);

        let a = Self::lerp(v00, v10, tx);
        let b = Self::lerp(v01, v11, tx);
        Self::lerp(a, b, tz)
    }

    /// Hash integer grid coordinates into deterministic noise in `[-1, 1]`.
    fn hash_2d(x: i32, z: i32) -> f32 {
        let mut n = x as u32;
        n = n
            .wrapping_mul(374761393)
            .wrapping_add((z as u32).wrapping_mul(668265263));
        n ^= n >> 13;
        n = n.wrapping_mul(1274126177);
        let v = (n & 0x00ff_ffff) as f32 / 0x00ff_ffff as f32;
        v * 2.0 - 1.0
    }

    /// Smooth interpolation curve used by value-noise blending.
    fn fade(t: f32) -> f32 {
        t * t * (3.0 - 2.0 * t)
    }

    /// Linearly interpolate between `a` and `b`.
    fn lerp(a: f32, b: f32, t: f32) -> f32 {
        a + (b - a) * t
    }

    /// Evaluate smoothstep between `edge0` and `edge1`.
    fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
        let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }
}
