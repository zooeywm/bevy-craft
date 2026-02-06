use bevy::prelude::*;

use crate::{BLOCK_SIZE, CHUNK_SIZE};

use crate::voxel::block_chunk::{Block, Chunk};
use crate::voxel::mesh::atlas::BlockAtlas;
use crate::voxel::mesh_types::{FACE_DEFS, FaceUv, FaceVertices, MeshData};

/// Build mesh data for all visible faces in one chunk.
///
/// For each solid block, this method iterates `FACE_DEFS`, culls hidden faces by
/// checking the neighbor block, and appends one quad per visible face.
pub(crate) fn build_chunk_mesh_data(chunk: &Chunk) -> MeshData {
    let mut positions: Vec<Vec3> = Vec::new();
    let mut normals: Vec<Vec3> = Vec::new();
    let mut uvs: Vec<Vec2> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    for z in 0..CHUNK_SIZE {
        for y in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let local = IVec3::new(x, y, z);
                let block = chunk.get_block(local);
                if block.is_air() {
                    continue;
                }
                let base = local.as_vec3() * BLOCK_SIZE;
                for face in FACE_DEFS {
                    let neighbor = local + face.neighbor;
                    // This face's neighbor isn't air, don't draw it.
                    if chunk.get_block(neighbor).is_solid() {
                        continue;
                    }
                    add_face(
                        &mut positions,
                        &mut normals,
                        &mut uvs,
                        &mut indices,
                        // Expand unit-cube corners into world-space quad vertices.
                        FaceVertices([
                            base + face.corners[0].as_vec3() * BLOCK_SIZE,
                            base + face.corners[1].as_vec3() * BLOCK_SIZE,
                            base + face.corners[2].as_vec3() * BLOCK_SIZE,
                            base + face.corners[3].as_vec3() * BLOCK_SIZE,
                        ]),
                        BlockAtlas::face_uvs_for_face(block, face.normal),
                        face.normal.as_vec3(),
                    );
                }
            }
        }
    }

    MeshData {
        positions,
        normals,
        uvs,
        indices,
    }
}

/// Convert intermediate mesh buffers into a Bevy `Mesh`.
pub(crate) fn mesh_from_data(data: MeshData) -> Mesh {
    let mut mesh = Mesh::new(
        bevy::render::render_resource::PrimitiveTopology::TriangleList,
        bevy::asset::RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, data.positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, data.normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, data.uvs);
    mesh.insert_indices(bevy::mesh::Indices::U32(data.indices));
    mesh
}

/// Append one quad face to mesh buffers as two indexed triangles.
///
/// The quad is emitted in the given vertex order and expanded into indices:
/// `(0, 1, 2)` and `(0, 2, 3)`.
fn add_face(
    positions: &mut Vec<Vec3>,
    normals: &mut Vec<Vec3>,
    uvs: &mut Vec<Vec2>,
    indices: &mut Vec<u32>,
    vertices: FaceVertices,
    uv: FaceUv,
    normal: Vec3,
) {
    // Emit one quad as two triangles via indexed vertices.
    let start = positions.len() as u32;
    positions.extend_from_slice(&vertices.0);
    normals.extend_from_slice(&[normal, normal, normal, normal]);
    uvs.extend_from_slice(&uv.0);
    indices.extend_from_slice(&[start, start + 1, start + 2, start, start + 2, start + 3]);
}

/// Build mesh data for a single block (used for in-hand preview).
pub(crate) fn build_single_block_mesh_data(block: Block) -> MeshData {
    let mut positions: Vec<Vec3> = Vec::new();
    let mut normals: Vec<Vec3> = Vec::new();
    let mut uvs: Vec<Vec2> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    let fx = 0.0;
    let fy = 0.0;
    let fz = 0.0;
    // +X (right) face
    add_face(
        &mut positions,
        &mut normals,
        &mut uvs,
        &mut indices,
        FaceVertices([
            Vec3::new(fx + BLOCK_SIZE, fy, fz),
            Vec3::new(fx + BLOCK_SIZE, fy + BLOCK_SIZE, fz),
            Vec3::new(fx + BLOCK_SIZE, fy + BLOCK_SIZE, fz + BLOCK_SIZE),
            Vec3::new(fx + BLOCK_SIZE, fy, fz + BLOCK_SIZE),
        ]),
        BlockAtlas::face_uvs_for_face(block, IVec3::new(1, 0, 0)),
        Vec3::new(1.0, 0.0, 0.0),
    );
    // -X (left) face
    add_face(
        &mut positions,
        &mut normals,
        &mut uvs,
        &mut indices,
        FaceVertices([
            Vec3::new(fx, fy, fz + BLOCK_SIZE),
            Vec3::new(fx, fy + BLOCK_SIZE, fz + BLOCK_SIZE),
            Vec3::new(fx, fy + BLOCK_SIZE, fz),
            Vec3::new(fx, fy, fz),
        ]),
        BlockAtlas::face_uvs_for_face(block, IVec3::new(-1, 0, 0)),
        Vec3::new(-1.0, 0.0, 0.0),
    );
    // +Y (top) face
    add_face(
        &mut positions,
        &mut normals,
        &mut uvs,
        &mut indices,
        FaceVertices([
            Vec3::new(fx, fy + BLOCK_SIZE, fz),
            Vec3::new(fx, fy + BLOCK_SIZE, fz + BLOCK_SIZE),
            Vec3::new(fx + BLOCK_SIZE, fy + BLOCK_SIZE, fz + BLOCK_SIZE),
            Vec3::new(fx + BLOCK_SIZE, fy + BLOCK_SIZE, fz),
        ]),
        BlockAtlas::face_uvs_for_face(block, IVec3::new(0, 1, 0)),
        Vec3::new(0.0, 1.0, 0.0),
    );
    // -Y (bottom) face
    add_face(
        &mut positions,
        &mut normals,
        &mut uvs,
        &mut indices,
        FaceVertices([
            Vec3::new(fx, fy, fz + BLOCK_SIZE),
            Vec3::new(fx, fy, fz),
            Vec3::new(fx + BLOCK_SIZE, fy, fz),
            Vec3::new(fx + BLOCK_SIZE, fy, fz + BLOCK_SIZE),
        ]),
        BlockAtlas::face_uvs_for_face(block, IVec3::new(0, -1, 0)),
        Vec3::new(0.0, -1.0, 0.0),
    );
    // +Z (front) face
    add_face(
        &mut positions,
        &mut normals,
        &mut uvs,
        &mut indices,
        FaceVertices([
            Vec3::new(fx + BLOCK_SIZE, fy, fz + BLOCK_SIZE),
            Vec3::new(fx + BLOCK_SIZE, fy + BLOCK_SIZE, fz + BLOCK_SIZE),
            Vec3::new(fx, fy + BLOCK_SIZE, fz + BLOCK_SIZE),
            Vec3::new(fx, fy, fz + BLOCK_SIZE),
        ]),
        BlockAtlas::face_uvs_for_face(block, IVec3::new(0, 0, 1)),
        Vec3::new(0.0, 0.0, 1.0),
    );
    // -Z (back) face
    add_face(
        &mut positions,
        &mut normals,
        &mut uvs,
        &mut indices,
        FaceVertices([
            Vec3::new(fx, fy, fz),
            Vec3::new(fx, fy + BLOCK_SIZE, fz),
            Vec3::new(fx + BLOCK_SIZE, fy + BLOCK_SIZE, fz),
            Vec3::new(fx + BLOCK_SIZE, fy, fz),
        ]),
        BlockAtlas::face_uvs_for_face(block, IVec3::new(0, 0, -1)),
        Vec3::new(0.0, 0.0, -1.0),
    );

    MeshData {
        positions,
        normals,
        uvs,
        indices,
    }
}

/// Build a Bevy mesh directly for a single block.
pub fn build_single_block_mesh(block: Block) -> Mesh {
    mesh_from_data(build_single_block_mesh_data(block))
}
