//! Cleanup helpers for updater-managed build workspaces.

use crate::state::{PersistedState, UpdateStatus};
use anyhow::{Context, Result};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

const HEAVY_WORKSPACE_DIRS: &[&str] = &["builder", "codex-app", "dist"];

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CleanupSummary {
    pub pruned_workspaces: usize,
}

pub fn prune_unreferenced_workspaces(
    workspace_root: &Path,
    state: &PersistedState,
) -> Result<CleanupSummary> {
    let workspaces_root = workspace_root.join("workspaces");
    if !workspaces_root.is_dir() {
        return Ok(CleanupSummary::default());
    }

    let protected = protected_workspaces(workspace_root, state);
    let mut summary = CleanupSummary::default();

    for entry in fs::read_dir(&workspaces_root)
        .with_context(|| format!("Failed to read {}", workspaces_root.display()))?
    {
        let entry = entry?;
        let workspace_dir = entry.path();
        if !entry.file_type()?.is_dir() || protected.contains(&workspace_dir) {
            continue;
        }

        let mut pruned = false;
        for heavy_dir in HEAVY_WORKSPACE_DIRS {
            let target = workspace_dir.join(heavy_dir);
            if target.exists() {
                fs::remove_dir_all(&target)
                    .with_context(|| format!("Failed to remove {}", target.display()))?;
                pruned = true;
            }
        }

        if directory_is_empty(&workspace_dir)? {
            fs::remove_dir(&workspace_dir)
                .with_context(|| format!("Failed to remove {}", workspace_dir.display()))?;
            pruned = true;
        }

        if pruned {
            summary.pruned_workspaces += 1;
        }
    }

    Ok(summary)
}

pub fn derive_workspace_dir(
    workspace_root: &Path,
    artifact_path: Option<&Path>,
) -> Option<PathBuf> {
    let artifact_path = artifact_path?;
    let workspaces_root = workspace_root.join("workspaces");
    if let Ok(relative) = artifact_path.strip_prefix(&workspaces_root) {
        if let Some(component) = relative.components().next() {
            return Some(workspaces_root.join(component.as_os_str()));
        }
    }

    derive_workspace_dir_from_any_workspaces_ancestor(artifact_path)
}

pub fn normalize_artifact_workspace_dir(workspace_root: &Path, state: &mut PersistedState) {
    state.artifact_paths.workspace_dir = state
        .artifact_paths
        .package_path
        .as_deref()
        .and_then(|path| derive_workspace_dir(workspace_root, Some(path)))
        .or_else(|| {
            state
                .artifact_paths
                .rollback_package_path
                .as_deref()
                .and_then(|path| derive_workspace_dir(workspace_root, Some(path)))
        })
        .or_else(|| {
            should_protect_explicit_workspace_dir(&state.status)
                .then(|| state.artifact_paths.workspace_dir.clone())
                .flatten()
        });
}

fn protected_workspaces(workspace_root: &Path, state: &PersistedState) -> BTreeSet<PathBuf> {
    let mut protected = BTreeSet::new();

    for artifact_path in [
        state.artifact_paths.package_path.as_deref(),
        state.artifact_paths.rollback_package_path.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        if let Some(workspace_dir) = derive_workspace_dir(workspace_root, Some(artifact_path)) {
            protected.insert(workspace_dir);
        }
    }

    if should_protect_explicit_workspace_dir(&state.status) {
        if let Some(workspace_dir) = state.artifact_paths.workspace_dir.clone() {
            protected.insert(workspace_dir);
        }
    }

    protected
}

fn should_protect_explicit_workspace_dir(status: &UpdateStatus) -> bool {
    matches!(
        status,
        UpdateStatus::PreparingWorkspace
            | UpdateStatus::PatchingApp
            | UpdateStatus::BuildingPackage
            | UpdateStatus::Failed
    )
}

fn derive_workspace_dir_from_any_workspaces_ancestor(path: &Path) -> Option<PathBuf> {
    let mut child = path.to_path_buf();
    for ancestor in path.ancestors() {
        if ancestor
            .file_name()
            .is_some_and(|name| name == "workspaces")
        {
            return Some(child);
        }
        child = ancestor.to_path_buf();
    }
    None
}

fn directory_is_empty(path: &Path) -> Result<bool> {
    Ok(fs::read_dir(path)
        .with_context(|| format!("Failed to read {}", path.display()))?
        .next()
        .is_none())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{ArtifactPaths, PersistedState, UpdateStatus};
    use anyhow::Result;
    use std::{fs, path::PathBuf};

    fn create_workspace(root: &std::path::Path, name: &str) -> Result<PathBuf> {
        let workspace = root.join("workspaces").join(name);
        fs::create_dir_all(workspace.join("builder"))?;
        fs::create_dir_all(workspace.join("codex-app"))?;
        fs::create_dir_all(workspace.join("dist"))?;
        fs::create_dir_all(workspace.join("logs"))?;
        fs::create_dir_all(workspace.join("reports"))?;
        fs::write(workspace.join("builder/build.txt"), b"builder")?;
        fs::write(workspace.join("codex-app/app.txt"), b"app")?;
        fs::write(workspace.join("dist/pkg.deb"), b"pkg")?;
        fs::write(workspace.join("logs/install.log"), b"log")?;
        fs::write(workspace.join("reports/rebuild-report.json"), b"{}")?;
        fs::write(workspace.join("metadata.json"), b"{}")?;
        Ok(workspace)
    }

    #[test]
    fn referenced_package_workspace_is_not_pruned() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let workspace = create_workspace(temp.path(), "2026.05.19.131017+6d440c71")?;
        let package_path = workspace.join("dist/pkg.deb");

        let mut state = PersistedState::new(true);
        state.artifact_paths.package_path = Some(package_path);

        let summary = prune_unreferenced_workspaces(temp.path(), &state)?;

        assert_eq!(summary.pruned_workspaces, 0);
        assert!(workspace.join("builder").exists());
        assert!(workspace.join("codex-app").exists());
        assert!(workspace.join("dist").exists());
        Ok(())
    }

    #[test]
    fn rollback_workspace_is_not_pruned() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let workspace = create_workspace(temp.path(), "2026.05.18.010207+6d440c71")?;
        let rollback_path = workspace.join("dist/pkg.deb");

        let mut state = PersistedState::new(true);
        state.artifact_paths.rollback_package_path = Some(rollback_path);

        let summary = prune_unreferenced_workspaces(temp.path(), &state)?;

        assert_eq!(summary.pruned_workspaces, 0);
        assert!(workspace.join("builder").exists());
        assert!(workspace.join("codex-app").exists());
        assert!(workspace.join("dist").exists());
        Ok(())
    }

    #[test]
    fn unreferenced_workspace_prunes_heavy_artifacts_and_keeps_debug_files() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let workspace = create_workspace(temp.path(), "2026.05.17.120457+6d440c71")?;
        let state = PersistedState::new(true);

        let summary = prune_unreferenced_workspaces(temp.path(), &state)?;

        assert_eq!(summary.pruned_workspaces, 1);
        assert!(!workspace.join("builder").exists());
        assert!(!workspace.join("codex-app").exists());
        assert!(!workspace.join("dist").exists());
        assert!(workspace.join("logs/install.log").exists());
        assert!(workspace.join("reports/rebuild-report.json").exists());
        assert!(workspace.join("metadata.json").exists());
        Ok(())
    }

    #[test]
    fn empty_workspace_is_removed_after_prune() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let workspace_root = temp.path().join("workspaces");
        let workspace = workspace_root.join("2026.05.16.231927+6d440c71");
        fs::create_dir_all(workspace.join("builder"))?;
        fs::write(workspace.join("builder/build.txt"), b"builder")?;

        let state = PersistedState::new(true);
        let summary = prune_unreferenced_workspaces(temp.path(), &state)?;

        assert_eq!(summary.pruned_workspaces, 1);
        assert!(!workspace.exists());
        Ok(())
    }

    #[test]
    fn active_workspace_dir_is_protected_only_while_build_or_failed() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let workspace = create_workspace(temp.path(), "2026.05.15.233058+5937a9b4")?;
        let mut state = PersistedState::new(true);
        state.status = UpdateStatus::PatchingApp;
        state.artifact_paths.workspace_dir = Some(workspace.clone());

        let summary = prune_unreferenced_workspaces(temp.path(), &state)?;
        assert_eq!(summary.pruned_workspaces, 0);
        assert!(workspace.join("builder").exists());

        state.status = UpdateStatus::Installed;
        let summary = prune_unreferenced_workspaces(temp.path(), &state)?;
        assert_eq!(summary.pruned_workspaces, 1);
        assert!(!workspace.join("builder").exists());
        Ok(())
    }

    #[test]
    fn workspace_dir_is_derived_from_retained_package_path() {
        let workspace_root = PathBuf::from("/cache");
        let package_path =
            workspace_root.join("workspaces/2026.05.04.033705+b0c9ccab/dist/codex.deb");

        let derived = derive_workspace_dir(&workspace_root, Some(package_path.as_path()));

        assert_eq!(
            derived,
            Some(workspace_root.join("workspaces/2026.05.04.033705+b0c9ccab"))
        );
    }

    #[test]
    fn workspace_dir_is_not_derived_for_paths_outside_workspace_root() {
        let workspace_root = PathBuf::from("/cache");
        let package_path = PathBuf::from("/tmp/codex.deb");

        let derived = derive_workspace_dir(&workspace_root, Some(package_path.as_path()));

        assert_eq!(derived, None);
    }

    #[test]
    fn normalize_state_clears_stale_workspace_dir_for_superseded_candidate() {
        let workspace_root = PathBuf::from("/cache");
        let mut state = PersistedState::new(true);
        state.status = UpdateStatus::Installed;
        state.artifact_paths = ArtifactPaths {
            dmg_path: None,
            workspace_dir: Some(workspace_root.join("workspaces/2026.04.28.082247+abcdef12")),
            package_path: None,
            rollback_package_path: None,
        };

        normalize_artifact_workspace_dir(&workspace_root, &mut state);

        assert_eq!(state.artifact_paths.workspace_dir, None);
    }

    #[test]
    fn normalize_state_points_workspace_dir_at_rollback_package_when_available() {
        let workspace_root = PathBuf::from("/cache");
        let rollback_path = workspace_root.join(
            "workspaces/2026.05.01.010203+99999999/dist/codex-desktop-2026.05.01.010203-1-x86_64.pkg.tar.zst",
        );
        let mut state = PersistedState::new(true);
        state.status = UpdateStatus::Installed;
        state.artifact_paths = ArtifactPaths {
            dmg_path: None,
            workspace_dir: None,
            package_path: Some(rollback_path.clone()),
            rollback_package_path: Some(rollback_path),
        };

        normalize_artifact_workspace_dir(&workspace_root, &mut state);

        assert_eq!(
            state.artifact_paths.workspace_dir,
            Some(workspace_root.join("workspaces/2026.05.01.010203+99999999"))
        );
    }
}
