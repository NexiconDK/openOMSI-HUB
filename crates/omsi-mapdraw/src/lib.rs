//! The maps' own picture (openOMSI PRs #2782 and #2929, without the navigator's roads, which
//! the Omsi-Hub navigator draws itself): the palettes of the light and the dark look
//! (`style`), and the map's layers (`omsi_map::layers`, read off the map by
//! `omsi_scene::mapgen`) as triangles (`MapData`).

pub mod style;
mod mesh;

pub use mesh::{GroundLook, MapData, MODEL_TRIANGLES};
