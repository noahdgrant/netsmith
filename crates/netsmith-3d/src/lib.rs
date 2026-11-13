//! The _Scene_ of a board and what is made of it (ADR 0105, ADR 0107): the scene built from a
//! placed or routed [`Board`](netsmith_pcb::Board), its meshes, the glTF binary the editor's 3D
//! view displays, the STEP file for CAD, and netsmith's own rasterizer that draws PNG renders of
//! it.
//!
//! ```text
//! Board (netsmith_pcb) --scene3d::scene--> Scene --mesh::of_scene--> meshes
//!                                                  |--gltf::glb      (.glb)
//!                                                  |--step::step     (.step)
//!                                                  |--raster3d       (.png)
//!                                                  `--render         (.png, as `netsmith render` asks)
//! ```
//!
//! The board model, the footprints and the part bodies they carry (`body3d`) are in
//! `netsmith-pcb`; this crate depends on it, never the reverse (issue #518), so that a change to
//! the 3D code runs only its own tests and its dependents', not the placer's and the router's.

pub mod case;
pub mod gltf;
pub mod mesh;
pub mod raster3d;
pub mod render;
pub mod scene3d;
pub mod step;
pub mod system;
