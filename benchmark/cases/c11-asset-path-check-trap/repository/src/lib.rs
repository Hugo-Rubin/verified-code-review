//! Static asset serving.

pub(crate) mod assets;
pub mod serve;

pub use assets::AssetKind;
pub use serve::{AssetServer, Response};
