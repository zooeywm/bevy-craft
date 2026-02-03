use crate::CHUNK_SIZE;

pub fn height_at(x: i32, z: i32) -> i32 {
    let fx = x as f32 * 0.08;
    let fz = z as f32 * 0.08;
    let noise = fbm_2d(fx, fz);
    let base = 4.0;
    let amp = 6.0;
    let height = (base + noise * amp).round() as i32;
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
