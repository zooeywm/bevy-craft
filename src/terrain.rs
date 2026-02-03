use crate::CHUNK_SIZE;

const BASE_HEIGHT: f32 = 4.0;
const PLAIN_AMPLITUDE: f32 = 0.9;
const MOUNTAIN_AMPLITUDE: f32 = 1000.0;
const MOUNTAIN_WEIGHT: f32 = 0.8;
const TERRAIN_SCALE: f32 = 0.06;
const MOUNTAIN_SCALE: f32 = 0.18;

pub fn height_at(x: i32, z: i32) -> i32 {
    let fx = x as f32 * TERRAIN_SCALE;
    let fz = z as f32 * TERRAIN_SCALE;

    let noise = fbm_2d(fx, fz);
    let mask = (fbm_2d(fx * MOUNTAIN_SCALE, fz * MOUNTAIN_SCALE) + 1.0) * 0.5;
    let mountain_mask = mask.powf(2.0);
    let amp = lerp(PLAIN_AMPLITUDE, MOUNTAIN_AMPLITUDE, mountain_mask * MOUNTAIN_WEIGHT);
    let height = (BASE_HEIGHT + noise * amp).round() as i32;
    height.clamp(1, CHUNK_SIZE - 1)
}

fn fbm_2d(x: f32, z: f32) -> f32 {
    let mut value = 0.0;
    let mut amplitude = 1.0;
    let mut frequency = 1.0;
    let mut norm = 0.0;
    for _ in 0..3 {
        value += value_noise_2d(x * frequency, z * frequency) * amplitude;
        norm += amplitude;
        amplitude *= 0.5;
        frequency *= 2.0;
    }
    value / norm
}

fn value_noise_2d(x: f32, z: f32) -> f32 {
    let x0 = x.floor() as i32;
    let z0 = z.floor() as i32;
    let x1 = x0 + 1;
    let z1 = z0 + 1;
    let tx = fade(x - x0 as f32);
    let tz = fade(z - z0 as f32);

    let v00 = hash_2d(x0, z0);
    let v10 = hash_2d(x1, z0);
    let v01 = hash_2d(x0, z1);
    let v11 = hash_2d(x1, z1);

    let a = lerp(v00, v10, tx);
    let b = lerp(v01, v11, tx);
    lerp(a, b, tz)
}

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

fn fade(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
