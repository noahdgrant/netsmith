//! The _Scene_ of a board and what is made of it (ADR 0105, ADR 0107): the scene built from a
//! placed or routed [`Board`](netsmith_pcb::Board), its meshes, the glTF binary the editor's 3D
//! view displays, the STEP file for CAD, and netsmith's own rasterizer that draws PNG renders of
//! it.
//!
//! ```text
//! Board (netsmith_pcb) --Scene::of--> Scene --meshes_of_scene--> meshes
//!                                          |--glb          (.glb)
//!                                          |--scene_step   (.step)
//!                                          |--render_view  (.png)
//!                                          `--render_png   (.png, as `netsmith render` asks)
//! ```
//!
//! The modules are private; this root lists the crate's API. A name that would be too general
//! at the root carries what it belongs to (`CaseWhy`, `ScenePlan`, `SystemFinding`,
//! `RenderRequest`, `mesh_pieces`).
//!
//! The board model, the footprints and the part bodies they carry (`body3d`) are in
//! `netsmith-pcb`; this crate depends on it, never the reverse (issue #518), so that a change to
//! the 3D code runs only its own tests and its dependents', not the placer's and the router's.

#![cfg_attr(not(test), warn(unreachable_pub))]

mod case;
mod gltf;
mod mesh;
mod raster3d;
mod render;
mod scene3d;
mod step;
mod system;

pub use case::{
    CaseScene, FitFinding, FitKind, PlacedCase, Subject as CaseSubject, Weight as CaseWeight,
    Why as CaseWhy, check as check_case, place as place_case,
};
pub use gltf::{glb, json_chunk, system_glb};
pub use mesh::{
    Axis, Material, Mesh, NO_NET, Pieces as MeshPieces, Polygon as MeshPolygon, SILK_LIFT,
    bounds_of as mesh_bounds, circle as circle_polygon, of_scene as meshes_of_scene,
    of_scene_section as meshes_of_scene_section, pieces as mesh_pieces,
    rounded_rect as rounded_rect_polygon,
};
pub use raster3d::{
    Camera, Image, Lens, RenderOptions, Rgba, Section, Theme, View, render as render_meshes,
    render_view,
};
pub use render::{
    CaseMode as RenderCaseMode, DEFAULT_SIZE as DEFAULT_RENDER_SIZE,
    DEFAULT_VIEW as DEFAULT_RENDER_VIEW, Request as RenderRequest,
    default_out as default_render_out, draw as render_png, draw_system as render_system_png,
    parse_section as parse_render_section, parse_size as parse_render_size,
};
pub use scene3d::{
    Body as SceneBody, CopperLayer as SceneCopperLayer, CopperPad as SceneCopperPad,
    HeightProvenance, HoleOwner, ModelUse, Pins as ScenePins, Plan as ScenePlan,
    PlanArea as ScenePlanArea, PlanBox as ScenePlanBox, Scene, SceneCopper, Tallest,
};
pub use step::{
    read::{ReadModel as StepModel, load as load_step, read as read_step},
    step as scene_step, system_step,
};
pub use system::{
    Code as SystemCode, Finding as SystemFinding, MateInput, MemberInput, Place as SystemPlace,
    SystemScene,
};
