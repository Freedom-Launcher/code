//! One content catalog over Modrinth and CurseForge. CurseForge ids carry the
//! `cf:` prefix; everything else is Modrinth.

use crate::{
    curseforge as cf,
    modrinth::{self, Project, ProjectPage, Version},
    Core, CoreError,
};

/// A project id of either catalog.
pub fn valid_id(id: &str) -> bool {
    if cf::is_cf(id) {
        cf::num(id).is_some()
    } else {
        modrinth::valid_project_id(id)
    }
}

pub async fn project(core: &Core, id: &str) -> Result<Project, CoreError> {
    if cf::is_cf(id) {
        cf::project(core, id).await
    } else {
        modrinth::project(core, id).await
    }
}

pub async fn project_page(core: &Core, id: &str) -> Result<ProjectPage, CoreError> {
    if cf::is_cf(id) {
        cf::project_page(core, id).await
    } else {
        modrinth::project_page(core, id).await
    }
}

/// Versions for one game version and any of `loaders`, newest first.
pub async fn versions(
    core: &Core,
    id: &str,
    mc: &str,
    loaders: &[&str],
) -> Result<Vec<Version>, CoreError> {
    if cf::is_cf(id) {
        cf::versions(core, id, mc, loaders).await
    } else {
        modrinth::versions(core, id, mc, loaders).await
    }
}

/// Every version of a project (CurseForge: the newest few hundred), newest first.
pub async fn project_versions(core: &Core, id: &str) -> Result<Vec<Version>, CoreError> {
    if cf::is_cf(id) {
        cf::project_versions(core, id).await
    } else {
        modrinth::project_versions(core, id).await
    }
}

/// One version of `project`.
pub async fn version(core: &Core, project: &str, id: &str) -> Result<Version, CoreError> {
    if cf::is_cf(id) {
        cf::version(core, project, id).await
    } else {
        modrinth::version(core, id).await
    }
}