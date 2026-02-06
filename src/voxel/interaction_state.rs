use bevy::prelude::*;

use crate::player::PreviewBlock;
use crate::voxel::block_chunk::Block;
use crate::voxel::mesh::{build_single_block_mesh_data, mesh_from_data};

#[derive(Resource)]
/// Placement/preview selection state for the current block variant.
pub struct SelectedBlock {
    /// Block state currently selected for placement and preview.
    pub current: Block,
}

impl SelectedBlock {
    /// Construct selected-block state with an initial block choice.
    pub fn new(current: Block) -> Self {
        Self { current }
    }

    /// Hotkey for selecting grassed dirt block.
    const SELECT_BLOCK_KEY_1: KeyCode = KeyCode::Digit1;
    /// Hotkey for selecting plain dirt block.
    const SELECT_BLOCK_KEY_2: KeyCode = KeyCode::Digit2;
    /// Hotkey for selecting sand block.
    const SELECT_BLOCK_KEY_3: KeyCode = KeyCode::Digit3;

    /// Apply block-selection hotkeys and refresh preview mesh when selection changes.
    pub(crate) fn apply_hotkeys(
        &mut self,
        keys: &Res<ButtonInput<KeyCode>>,
        meshes: &mut ResMut<Assets<Mesh>>,
        preview_query: &mut Query<&mut bevy::mesh::Mesh3d, With<PreviewBlock>>,
    ) {
        if keys.just_pressed(Self::SELECT_BLOCK_KEY_1) {
            self.set_with_preview(Block::dirt_with_grass(), meshes, preview_query);
        }
        if keys.just_pressed(Self::SELECT_BLOCK_KEY_2) {
            self.set_with_preview(Block::dirt(), meshes, preview_query);
        }
        if keys.just_pressed(Self::SELECT_BLOCK_KEY_3) {
            self.set_with_preview(Block::sand(), meshes, preview_query);
        }
    }

    /// Set selected block and update preview mesh.
    fn set_with_preview(
        &mut self,
        block: Block,
        meshes: &mut ResMut<Assets<Mesh>>,
        preview_query: &mut Query<&mut bevy::mesh::Mesh3d, With<PreviewBlock>>,
    ) {
        self.current = block;
        self.update_preview_mesh(meshes, preview_query);
    }

    /// Update the preview mesh to match current selected block.
    fn update_preview_mesh(
        &self,
        meshes: &mut ResMut<Assets<Mesh>>,
        preview_query: &mut Query<&mut bevy::mesh::Mesh3d, With<PreviewBlock>>,
    ) {
        let Ok(mut mesh_handle) = preview_query.single_mut() else {
            return;
        };
        let new_mesh = meshes.add(mesh_from_data(build_single_block_mesh_data(self.current)));
        *mesh_handle = bevy::mesh::Mesh3d(new_mesh);
    }
}

#[derive(Resource)]
/// Cooldown timestamps for repeated break/place interactions.
pub struct InteractionCooldown {
    /// Last simulation time (seconds) when breaking was applied.
    pub last_break_time: f32,
    /// Last simulation time (seconds) when placing was applied.
    pub last_place_time: f32,
}

impl InteractionCooldown {
    /// Construct interaction cooldown state with "ready" timestamps.
    pub fn new() -> Self {
        Self {
            last_break_time: -1.0,
            last_place_time: -1.0,
        }
    }

    /// Interaction cooldown in seconds.
    const INTERACTION_COOLDOWN_SECS: f32 = 0.2;

    /// Return whether break interaction is currently allowed.
    pub(crate) fn can_break(&self, buttons: &ButtonInput<MouseButton>, time: &Time) -> bool {
        self.can_with_button(buttons, MouseButton::Left, self.last_break_time, time)
    }

    /// Return whether place interaction is currently allowed.
    pub(crate) fn can_place(&self, buttons: &ButtonInput<MouseButton>, time: &Time) -> bool {
        self.can_with_button(buttons, MouseButton::Right, self.last_place_time, time)
    }

    /// Record break action timestamp.
    pub(crate) fn mark_break(&mut self, time: &Time) {
        let now = Self::now(time);
        self.last_break_time = now;
    }

    /// Record place action timestamp.
    pub(crate) fn mark_place(&mut self, time: &Time) {
        let now = Self::now(time);
        self.last_place_time = now;
    }

    /// Read current elapsed seconds from Bevy time resource.
    fn now(time: &Time) -> f32 {
        time.elapsed_secs()
    }

    /// Generic cooldown gate for one mouse button and last-trigger timestamp.
    fn can_with_button(
        &self,
        buttons: &ButtonInput<MouseButton>,
        button: MouseButton,
        last_time: f32,
        time: &Time,
    ) -> bool {
        let now = Self::now(time);
        buttons.pressed(button) && now - last_time >= Self::INTERACTION_COOLDOWN_SECS
    }
}
