use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    os::unix::fs as unix_fs,
    path::{Component, Path, PathBuf},
};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SkillInspection {
    pub ok: bool,
    pub command: &'static str,
    pub skill_path: PathBuf,
    pub name: Option<String>,
    pub description: Option<String>,
    pub status: SkillStatus,
    pub capabilities: Vec<SkillCapability>,
    pub blockers: Vec<String>,
    pub warnings: Vec<String>,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillStatus {
    Supported,
    Conditional,
    Experimental,
    Unsupported,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SkillCapability {
    InstructionOnly,
    CliLocal,
    BrowserSession,
    PluginDependent,
    DesktopObserve,
    DesktopAct,
    IsolatedGui,
    PlatformMacos,
    PlatformWindows,
    Recording,
}

#[derive(Debug, Clone)]
pub struct SkillImportOptions {
    pub source: PathBuf,
    pub target: ImportTarget,
    pub target_dir: Option<PathBuf>,
    pub mode: ImportMode,
    pub dry_run: bool,
    pub allow_unsupported: bool,
    pub overwrite: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportTarget {
    User,
    Repo,
    Explicit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportMode {
    Copy,
    Symlink,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SkillImportReport {
    pub ok: bool,
    pub command: &'static str,
    pub dry_run: bool,
    pub source: PathBuf,
    pub destination: PathBuf,
    pub inspection: SkillInspection,
    pub imported: bool,
    pub warnings: Vec<String>,
}

pub fn inspect_skill(source: &Path) -> Result<SkillInspection> {
    let skill_dir = source
        .canonicalize()
        .with_context(|| format!("failed to canonicalize {}", source.display()))?;
    if !skill_dir.is_dir() {
        bail!("skill source is not a directory: {}", source.display());
    }
    let skill_md = skill_dir.join("SKILL.md");
    if !skill_md.is_file() {
        bail!("skill source is missing SKILL.md: {}", skill_dir.display());
    }
    let skill_md_text = fs::read_to_string(&skill_md)
        .with_context(|| format!("failed to read {}", skill_md.display()))?;
    let (name, description) = parse_frontmatter(&skill_md_text);
    let mut inspection = SkillInspection {
        ok: true,
        command: "skill.inspect",
        skill_path: skill_dir.clone(),
        name,
        description,
        status: SkillStatus::Unknown,
        capabilities: Vec::new(),
        blockers: Vec::new(),
        warnings: Vec::new(),
        evidence: Vec::new(),
    };

    if inspection.name.is_none() {
        inspection
            .blockers
            .push("SKILL.md frontmatter is missing name".to_string());
    }
    if inspection.description.is_none() {
        inspection
            .blockers
            .push("SKILL.md frontmatter is missing description".to_string());
    }

    scan_skill_tree(&skill_dir, &mut inspection)?;
    classify_text(&skill_md_text, &mut inspection);

    if inspection.capabilities.is_empty() {
        inspection
            .capabilities
            .push(SkillCapability::InstructionOnly);
    }
    inspection.status = status_for_capabilities(&inspection.capabilities);
    inspection.ok = inspection.blockers.is_empty();
    Ok(inspection)
}

pub fn import_skill(options: SkillImportOptions) -> Result<SkillImportReport> {
    let inspection = inspect_skill(&options.source)?;
    if !options.allow_unsupported && inspection.status == SkillStatus::Unsupported {
        bail!(
            "skill is unsupported on Linux; re-run with --allow-unsupported to import as context"
        );
    }
    if !inspection.blockers.is_empty() {
        bail!(
            "skill failed safety inspection: {}",
            inspection.blockers.join("; ")
        );
    }

    let destination = import_destination(&options, &inspection)?;
    let mut warnings = inspection.warnings.clone();
    if destination.exists() && !options.overwrite {
        bail!("target skill already exists: {}", destination.display());
    }
    if matches!(options.mode, ImportMode::Symlink) {
        warnings
            .push("symlink import keeps the external source live; review before use".to_string());
    }

    if !options.dry_run {
        fs::create_dir_all(
            destination
                .parent()
                .ok_or_else(|| anyhow!("destination has no parent: {}", destination.display()))?,
        )?;
        if destination.exists() && options.overwrite {
            fs::remove_dir_all(&destination)?;
        }
        match options.mode {
            ImportMode::Copy => copy_skill_dir(&inspection.skill_path, &destination)?,
            ImportMode::Symlink => unix_fs::symlink(&inspection.skill_path, &destination)
                .with_context(|| format!("failed to symlink {}", destination.display()))?,
        }
    }

    Ok(SkillImportReport {
        ok: true,
        command: "skill.import",
        dry_run: options.dry_run,
        source: inspection.skill_path.clone(),
        destination,
        inspection,
        imported: !options.dry_run,
        warnings,
    })
}

fn scan_skill_tree(skill_dir: &Path, inspection: &mut SkillInspection) -> Result<()> {
    let mut stack = vec![skill_dir.to_path_buf()];
    let mut files_seen = 0usize;
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            let relative = path.strip_prefix(skill_dir).unwrap_or(&path);
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.file_type().is_symlink() {
                inspection.blockers.push(format!(
                    "internal symlink is not imported by default: {}",
                    relative.display()
                ));
                continue;
            }
            if metadata.is_dir() {
                stack.push(path);
                continue;
            }
            if !metadata.is_file() {
                inspection.blockers.push(format!(
                    "non-regular file is not allowed: {}",
                    relative.display()
                ));
                continue;
            }
            files_seen += 1;
            if files_seen > 512 {
                inspection
                    .blockers
                    .push("skill scan stopped after 512 files".to_string());
                return Ok(());
            }
            if metadata.len() > 256 * 1024 {
                inspection.blockers.push(format!(
                    "large file is too large to inspect: {}",
                    relative.display()
                ));
                continue;
            }
            if is_probably_executable(&metadata, &path) {
                push_capability(inspection, SkillCapability::CliLocal);
                inspection.warnings.push(format!(
                    "executable/script file present: {}",
                    relative.display()
                ));
                inspection
                    .evidence
                    .push(format!("script:{}", relative.display()));
            }
            if let Some(ext) = path.extension().and_then(|ext| ext.to_str()) {
                if matches!(ext, "sh" | "bash" | "py" | "js" | "mjs" | "ts") {
                    push_capability(inspection, SkillCapability::CliLocal);
                }
            }
        }
    }
    Ok(())
}

fn classify_text(text: &str, inspection: &mut SkillInspection) {
    let lower = text.to_lowercase();
    for (needle, capability, evidence) in [
        (
            "applescript",
            SkillCapability::PlatformMacos,
            "mentions AppleScript",
        ),
        ("finder", SkillCapability::PlatformMacos, "mentions Finder"),
        (
            ".app",
            SkillCapability::PlatformMacos,
            "mentions macOS app bundle",
        ),
        (
            "keychain",
            SkillCapability::PlatformMacos,
            "mentions Keychain",
        ),
        (
            "powershell",
            SkillCapability::PlatformWindows,
            "mentions PowerShell",
        ),
        (
            "registry",
            SkillCapability::PlatformWindows,
            "mentions Windows registry",
        ),
        (
            "chrome",
            SkillCapability::BrowserSession,
            "mentions browser session",
        ),
        (
            "browser",
            SkillCapability::BrowserSession,
            "mentions browser session",
        ),
        (
            "plugin",
            SkillCapability::PluginDependent,
            "mentions plugin dependency",
        ),
        (
            "mcp",
            SkillCapability::PluginDependent,
            "mentions MCP dependency",
        ),
        (
            "screenshot",
            SkillCapability::DesktopObserve,
            "mentions screenshot",
        ),
        (
            "accessibility",
            SkillCapability::DesktopObserve,
            "mentions accessibility",
        ),
        (
            "click",
            SkillCapability::DesktopAct,
            "mentions click action",
        ),
        ("type", SkillCapability::DesktopAct, "mentions type action"),
        ("drag", SkillCapability::DesktopAct, "mentions drag action"),
        (
            "record a skill",
            SkillCapability::Recording,
            "mentions new recording",
        ),
    ] {
        if lower.contains(needle) {
            push_capability(inspection, capability);
            inspection.evidence.push(evidence.to_string());
        }
    }
}

fn status_for_capabilities(capabilities: &[SkillCapability]) -> SkillStatus {
    if capabilities.iter().any(|capability| {
        matches!(
            capability,
            SkillCapability::PlatformMacos
                | SkillCapability::PlatformWindows
                | SkillCapability::Recording
        )
    }) {
        SkillStatus::Unsupported
    } else if capabilities.iter().any(|capability| {
        matches!(
            capability,
            SkillCapability::DesktopAct
                | SkillCapability::DesktopObserve
                | SkillCapability::IsolatedGui
        )
    }) {
        SkillStatus::Experimental
    } else if capabilities
        .iter()
        .any(|capability| !matches!(capability, SkillCapability::InstructionOnly))
    {
        SkillStatus::Conditional
    } else {
        SkillStatus::Supported
    }
}

fn import_destination(
    options: &SkillImportOptions,
    inspection: &SkillInspection,
) -> Result<PathBuf> {
    let root = match options.target {
        ImportTarget::User => home_dir()
            .ok_or_else(|| anyhow!("HOME is not set"))?
            .join(".agents")
            .join("skills"),
        ImportTarget::Repo => std::env::current_dir()?.join(".agents").join("skills"),
        ImportTarget::Explicit => options
            .target_dir
            .clone()
            .ok_or_else(|| anyhow!("--target-dir is required for explicit target"))?,
    };
    let name = inspection
        .name
        .as_deref()
        .map(safe_skill_dir_name)
        .filter(|name| !name.is_empty())
        .or_else(|| {
            inspection
                .skill_path
                .file_name()
                .and_then(|name| name.to_str())
                .map(safe_skill_dir_name)
        })
        .filter(|name| !name.is_empty())
        .ok_or_else(|| anyhow!("could not derive safe skill directory name"))?;
    if !is_direct_child_name(&name) {
        bail!("could not derive safe skill directory name");
    }
    Ok(root.join(name))
}

fn copy_skill_dir(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)?;
    set_normalized_dir_mode(destination)?;
    let mut stack = vec![source.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)?;
            let relative = path.strip_prefix(source)?;
            let target = destination.join(relative);
            if metadata.file_type().is_symlink() {
                bail!("refusing to copy internal symlink: {}", relative.display());
            }
            if metadata.is_dir() {
                fs::create_dir_all(&target)?;
                set_normalized_dir_mode(&target)?;
                stack.push(path);
            } else if metadata.is_file() {
                fs::copy(&path, &target)
                    .with_context(|| format!("failed to copy {}", relative.display()))?;
                set_normalized_file_mode(&target, copy_as_executable(&metadata, &path))?;
            } else {
                bail!("refusing to copy non-regular file: {}", relative.display());
            }
        }
    }
    Ok(())
}

fn parse_frontmatter(text: &str) -> (Option<String>, Option<String>) {
    if !text.starts_with("---\n") {
        return (None, None);
    }
    let Some(end) = text[4..].find("\n---") else {
        return (None, None);
    };
    let frontmatter = &text[4..(4 + end)];
    let mut name = None;
    let mut description = None;
    for line in frontmatter.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value
            .trim()
            .trim_matches('"')
            .trim_matches('\'')
            .to_string();
        match key.trim() {
            "name" => name = Some(value),
            "description" => description = Some(value),
            _ => {}
        }
    }
    (name, description)
}

fn push_capability(inspection: &mut SkillInspection, capability: SkillCapability) {
    if !inspection.capabilities.contains(&capability) {
        inspection.capabilities.push(capability);
    }
}

fn safe_skill_dir_name(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

fn is_direct_child_name(value: &str) -> bool {
    let mut components = Path::new(value).components();
    matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none()
}

fn set_normalized_dir_mode(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))
            .with_context(|| format!("failed to chmod {}", path.display()))?;
    }
    Ok(())
}

fn set_normalized_file_mode(path: &Path, executable: bool) -> Result<()> {
    #[cfg(unix)]
    {
        let mode = if executable { 0o755 } else { 0o644 };
        fs::set_permissions(path, fs::Permissions::from_mode(mode))
            .with_context(|| format!("failed to chmod {}", path.display()))?;
    }
    Ok(())
}

fn copy_as_executable(metadata: &fs::Metadata, path: &Path) -> bool {
    if path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("SKILL.md"))
    {
        return false;
    }
    source_has_execute_bits(metadata)
}

#[cfg(unix)]
fn source_has_execute_bits(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn source_has_execute_bits(metadata: &fs::Metadata) -> bool {
    !metadata.permissions().readonly()
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

#[cfg(unix)]
fn is_probably_executable(metadata: &fs::Metadata, path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o111 != 0
        || path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| matches!(ext, "sh" | "bash" | "py" | "js" | "mjs" | "ts"))
}

#[cfg(not(unix))]
fn is_probably_executable(_metadata: &fs::Metadata, path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| matches!(ext, "sh" | "bash" | "py" | "js" | "mjs" | "ts"))
}
