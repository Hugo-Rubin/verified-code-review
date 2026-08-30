//! The asset-serving endpoint.

use crate::assets::{asset_path, AssetKind};
use std::path::{Path, PathBuf};

#[derive(Debug, PartialEq, Eq)]
pub enum Response {
    File(PathBuf),
    NotFound,
}

pub struct AssetServer {
    root: PathBuf,
}

impl AssetServer {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Serve one of the known assets.
    pub fn serve(&self, kind: AssetKind) -> Response {
        match asset_path(&self.root, kind.file_name()) {
            Some(path) => Response::File(path),
            None => Response::NotFound,
        }
    }

    /// Map a request path onto a known asset, if it names one.
    pub fn route(&self, request_path: &str) -> Response {
        let kind = match request_path {
            "/static/app.css" => AssetKind::Stylesheet,
            "/favicon.ico" => AssetKind::Favicon,
            "/static/logo.svg" => AssetKind::Logo,
            _ => return Response::NotFound,
        };
        self.serve(kind)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serves_a_known_asset() {
        let s = AssetServer::new("/srv/assets");
        assert_eq!(
            s.serve(AssetKind::Favicon),
            Response::File(PathBuf::from("/srv/assets").join("favicon.ico"))
        );
    }

    #[test]
    fn routes_known_request_paths() {
        let s = AssetServer::new("/srv/assets");
        assert!(matches!(s.route("/favicon.ico"), Response::File(_)));
    }

    #[test]
    fn unknown_request_paths_are_not_found() {
        let s = AssetServer::new("/srv/assets");
        assert_eq!(s.route("/../../etc/passwd"), Response::NotFound);
        assert_eq!(s.route("/static/secrets.env"), Response::NotFound);
    }
}
