/// Shared texture ids used by both runtime atlas sampling and atlas builder tool.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextureId {
    /// Left/right texture for grass-over-dirt blocks.
    GrassSide,
    /// Top texture for grass-over-dirt blocks.
    GrassTop,
    /// Dirt texture.
    Dirt,
    /// Sand texture.
    Sand,
}

/// Stable atlas tile order used by runtime UV lookup and atlas generation.
pub const ATLAS_TEXTURE_ORDER: [TextureId; 4] = [
    TextureId::GrassSide,
    TextureId::GrassTop,
    TextureId::Dirt,
    TextureId::Sand,
];

/// Return atlas tile order as a slice.
#[allow(dead_code, reason = "used by atlas tool binary")]
pub const fn atlas_texture_order() -> &'static [TextureId] {
    &ATLAS_TEXTURE_ORDER
}

/// Return the source texture file name for one texture id.
#[allow(dead_code, reason = "used by atlas tool binary")]
pub const fn source_base_filename(texture: TextureId) -> &'static str {
    match texture {
        TextureId::GrassSide => "default_dirt.png",
        TextureId::GrassTop => "default_grass.png",
        TextureId::Dirt => "default_dirt.png",
        TextureId::Sand => "default_sand.png",
    }
}

/// Return optional overlay texture file name for one texture id.
#[allow(dead_code, reason = "used by atlas tool binary")]
pub const fn source_overlay_filename(texture: TextureId) -> Option<&'static str> {
    match texture {
        TextureId::GrassSide => Some("default_grass_side.png"),
        TextureId::GrassTop => None,
        TextureId::Dirt => None,
        TextureId::Sand => None,
    }
}

/// Return horizontal tile count of the current atlas.
#[allow(dead_code, reason = "used by runtime mesh atlas")]
pub fn atlas_tiles_x() -> f32 {
    ATLAS_TEXTURE_ORDER.len() as f32
}

/// Return tile index in the horizontal atlas for one texture id.
#[allow(dead_code, reason = "used by runtime mesh atlas")]
pub const fn atlas_tile_index(texture: TextureId) -> u32 {
    match texture {
        TextureId::GrassSide => 0,
        TextureId::GrassTop => 1,
        TextureId::Dirt => 2,
        TextureId::Sand => 3,
    }
}

/// Return whether this texture should use V-flipped UVs.
#[allow(dead_code, reason = "used by runtime mesh atlas")]
pub const fn needs_v_flip(texture: TextureId) -> bool {
    matches!(texture, TextureId::GrassSide)
}
