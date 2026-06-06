use std::path::{Component, Path, PathBuf};

/// Join `name` under `base`, rejecting path traversal and absolute segments.
pub fn safe_join(base: &Path, name: &str) -> Result<PathBuf, String> {
    if name.is_empty() {
        return Err("name must not be empty".to_string());
    }
    if name.contains('\0') {
        return Err("name contains a null byte".to_string());
    }
    if name.starts_with('/') || name.starts_with('\\') {
        return Err("name must be relative".to_string());
    }

    let rel = Path::new(name);
    for component in rel.components() {
        match component {
            Component::ParentDir => return Err("name must not contain '..'".to_string()),
            Component::RootDir | Component::Prefix(_) => {
                return Err("name must be relative".to_string());
            }
            Component::CurDir | Component::Normal(_) => {}
        }
    }

    let joined = base.join(rel);
    let canonical_base = canonicalize_or_create(base)?;
    let canonical_joined = if joined.exists() {
        joined
            .canonicalize()
            .map_err(|e| format!("could not resolve path: {e}"))?
    } else {
        // Resolve the parent chain; the leaf may not exist yet.
        let parent = joined
            .parent()
            .ok_or_else(|| "invalid path".to_string())?;
        if !parent.as_os_str().is_empty() {
            let _ = std::fs::create_dir_all(parent);
        }
        let canonical_parent = if parent.as_os_str().is_empty() {
            canonical_base.clone()
        } else {
            parent
                .canonicalize()
                .map_err(|e| format!("could not resolve parent: {e}"))?
        };
        let file_name = joined
            .file_name()
            .ok_or_else(|| "invalid path".to_string())?;
        canonical_parent.join(file_name)
    };

    if !canonical_joined.starts_with(&canonical_base) {
        return Err("path escapes the base directory".to_string());
    }
    Ok(canonical_joined)
}

/// Reject paths with traversal components before filesystem operations.
pub fn validate_create_path(path: &str) -> Result<(), String> {
    if path.contains('\0') {
        return Err("path contains a null byte".to_string());
    }
    for component in Path::new(path).components() {
        if matches!(component, Component::ParentDir) {
            return Err("path must not contain '..'".to_string());
        }
    }
    Ok(())
}

fn canonicalize_or_create(base: &Path) -> Result<PathBuf, String> {
    if base.exists() {
        return base
            .canonicalize()
            .map_err(|e| format!("could not resolve base directory: {e}"));
    }
    std::fs::create_dir_all(base).map_err(|e| format!("could not create base directory: {e}"))?;
    base.canonicalize()
        .map_err(|e| format!("could not resolve base directory: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn safe_join_normal_file() {
        let dir = std::env::temp_dir().join("litecast_safe_join_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = safe_join(&dir, "notes.md").unwrap();
        assert!(path.ends_with("notes.md"));
        assert!(path.starts_with(dir.canonicalize().unwrap()));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn safe_join_nested_path() {
        let dir = std::env::temp_dir().join("litecast_safe_join_nested");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = safe_join(&dir, "a/b/c.txt").unwrap();
        assert!(path.ends_with("c.txt"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn safe_join_rejects_traversal() {
        let dir = std::env::temp_dir().join("litecast_safe_join_trav");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        assert!(safe_join(&dir, "../etc/passwd").is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn safe_join_rejects_absolute() {
        let dir = std::env::temp_dir().join("litecast_safe_join_abs");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        assert!(safe_join(&dir, "/etc/passwd").is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn validate_create_path_rejects_parent_dir() {
        assert!(validate_create_path("/tmp/foo/../bar").is_err());
        assert!(validate_create_path("/tmp/foo/bar").is_ok());
    }
}
