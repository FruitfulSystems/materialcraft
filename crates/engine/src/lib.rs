//! # materialcraft-engine
//!
//! The MaterialCraft session, L3: a [`Session`] owns the package, the graph being edited, the
//! selection, the 2D/3D view state, snapshot undo and the cook cache. Every user-visible action
//! is a command in the [`commands`] registry, dispatched by id from the UI, the CLI, the control
//! channel and the MCP server alike.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod commands;
pub mod edit_cmds;
pub mod file_cmds;
pub mod graph_cmds;
pub mod inspect;
pub mod material_cmds;
pub mod node_cmds;
pub mod samples;
pub mod session;
pub mod view_cmds;
pub mod wire_cmds;

pub use commands::{CommandSpec, command_specs, execute, find};
pub use session::{Camera3d, CameraPrefs, Clip, Display3d, Lights3d, Material3d, Renderer3d, Session, View2d, View3d, WARNING_PREFIX};

pub use materialcraft_core as core;
pub use materialcraft_eval as eval;
pub use materialcraft_formats as formats;
pub use materialcraft_geom as geom;
pub use materialcraft_image as image;
pub use materialcraft_nodes as nodes;

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("unknown command `{0}`")]
    UnknownCommand(String),
    #[error("`{cmd}` is not available: {why}")]
    Disabled { cmd: String, why: String },
    #[error("`{cmd}`: {msg}")]
    BadParams { cmd: String, msg: String },
    #[error(transparent)]
    Core(#[from] materialcraft_core::CoreError),
    #[error(transparent)]
    Format(#[from] materialcraft_formats::FormatError),
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, EngineError>;

/// Version string shown in About, `--version` and window titles.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// A fresh, unsaved package with one empty graph, used at desktop and browser startup.
pub fn new_session() -> Session {
    let mut pkg = core::Package::new("Untitled package");
    pkg.graphs.push(samples::template("empty", "Material_Graph", 10));
    Session::new(pkg)
}

/// A session on the built-in sample package (the brick wall).
pub fn sample_session() -> Session {
    Session::new(samples::sample_package())
}
