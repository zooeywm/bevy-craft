use bevy::prelude::*;

/// Raw mesh buffers assembled before uploading to a Bevy `Mesh`.
pub struct MeshData {
    /// Vertex positions in world/chunk mesh space (`Vec<Vec3>`).
    pub(crate) positions: Vec<Vec3>,
    /// Per-vertex normals used by lighting (`Vec<Vec3>`).
    pub(crate) normals: Vec<Vec3>,
    /// Per-vertex UV coordinates for texture atlas sampling (`Vec<Vec2>`).
    pub(crate) uvs: Vec<Vec2>,
    /// Triangle index buffer (u32).
    pub(crate) indices: Vec<u32>,
}

/// Table row describing one cube face for mesh generation.
///
/// A `FaceDef` captures everything needed to emit one quad:
/// - which neighbor voxel decides visibility (`neighbor`)
/// - which way the face points (`normal`)
/// - where the 4 quad corners are in unit-cube space (`corners`)
pub(crate) struct FaceDef {
    /// Face normal for lighting and face-tile rule lookup.
    pub(crate) normal: IVec3,
    /// Neighbor offset used to test whether a face is exposed.
    pub(crate) neighbor: IVec3,
    /// Quad corners in local unit-cube coordinates.
    pub(crate) corners: [IVec3; 4],
}

/// UV payload for one quad face in vertex order.
pub(crate) struct FaceUv(
    /// Face UV coordinates in quad-vertex order.
    pub(crate) [Vec2; 4],
);

/// Vertex payload for one quad face in vertex order.
pub(crate) struct FaceVertices(
    /// Face vertex positions in quad-vertex order.
    pub(crate) [Vec3; 4],
);

/// Static cube-face table used by `build_chunk_mesh_data`.
///
/// Order does not affect correctness; each entry fully describes one face.
pub(crate) const FACE_DEFS: [FaceDef; 6] = [
    // +X (right) face
    FaceDef {
        normal: IVec3::new(1, 0, 0),
        neighbor: IVec3::new(1, 0, 0),
        corners: [
            IVec3::new(1, 0, 0),
            IVec3::new(1, 1, 0),
            IVec3::new(1, 1, 1),
            IVec3::new(1, 0, 1),
        ],
    },
    // -X (left) face
    FaceDef {
        normal: IVec3::new(-1, 0, 0),
        neighbor: IVec3::new(-1, 0, 0),
        corners: [
            IVec3::new(0, 0, 1),
            IVec3::new(0, 1, 1),
            IVec3::new(0, 1, 0),
            IVec3::new(0, 0, 0),
        ],
    },
    // +Y (top) face
    FaceDef {
        normal: IVec3::new(0, 1, 0),
        neighbor: IVec3::new(0, 1, 0),
        corners: [
            IVec3::new(0, 1, 0),
            IVec3::new(0, 1, 1),
            IVec3::new(1, 1, 1),
            IVec3::new(1, 1, 0),
        ],
    },
    // -Y (bottom) face
    FaceDef {
        normal: IVec3::new(0, -1, 0),
        neighbor: IVec3::new(0, -1, 0),
        corners: [
            IVec3::new(0, 0, 1),
            IVec3::new(0, 0, 0),
            IVec3::new(1, 0, 0),
            IVec3::new(1, 0, 1),
        ],
    },
    // +Z (front) face
    FaceDef {
        normal: IVec3::new(0, 0, 1),
        neighbor: IVec3::new(0, 0, 1),
        corners: [
            IVec3::new(1, 0, 1),
            IVec3::new(1, 1, 1),
            IVec3::new(0, 1, 1),
            IVec3::new(0, 0, 1),
        ],
    },
    // -Z (back) face
    FaceDef {
        normal: IVec3::new(0, 0, -1),
        neighbor: IVec3::new(0, 0, -1),
        corners: [
            IVec3::new(0, 0, 0),
            IVec3::new(0, 1, 0),
            IVec3::new(1, 1, 0),
            IVec3::new(1, 0, 0),
        ],
    },
];
