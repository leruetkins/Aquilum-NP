use std::fs;
use std::path::{Path, PathBuf};

/// Copies a directory and all of its contents recursively.
pub fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let dest_path = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dest_path)?;
        } else {
            fs::copy(entry.path(), dest_path)?;
        }
    }
    Ok(())
}

/// Creates a new Markdown note inside the specified folder.
///
/// `untitled` is the default file stem in the current UI language
/// (callers pass the localized `common.untitled`). Collisions resolve as
/// `<untitled>.md`, `<untitled> 1.md`, etc.
pub fn create_note_in_folder(folder: &Path, untitled: &str) -> Result<PathBuf, String> {
    if !folder.exists() {
        return Err(format!("Папка не существует: {:?}", folder));
    }

    let mut candidate = folder.join(format!("{}.md", untitled));
    let mut counter = 1;
    while candidate.exists() {
        candidate = folder.join(format!("{} {}.md", untitled, counter));
        counter += 1;
    }

    let title = candidate
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| untitled.to_string());

    let initial_content = format!("# {}\n\n", title);
    fs::write(&candidate, initial_content).map_err(|e| format!("Ошибка создания заметки: {}", e))?;

    Ok(candidate)
}

/// Creates a new subfolder inside the specified folder.
/// Automatically handles naming collisions (`Новая папка`, `Новая папка 1`, etc.).
pub fn create_folder_in_folder(folder: &Path) -> Result<PathBuf, String> {
    if !folder.exists() {
        return Err(format!("Папка не существует: {:?}", folder));
    }

    let mut candidate = folder.join("Новая папка");
    let mut counter = 1;
    while candidate.exists() {
        candidate = folder.join(format!("Новая папка {}", counter));
        counter += 1;
    }

    fs::create_dir_all(&candidate).map_err(|e| format!("Ошибка создания папки: {}", e))?;
    Ok(candidate)
}

/// Creates a new Canvas file inside the specified folder.
///
/// `untitled` is the default file stem in the current UI language
/// (callers pass the localized `common.untitled`).
pub fn create_canvas_in_folder(folder: &Path, untitled: &str) -> Result<PathBuf, String> {
    if !folder.exists() {
        return Err(format!("Папка не существует: {:?}", folder));
    }

    let mut candidate = folder.join(format!("{}.canvas", untitled));
    let mut counter = 1;
    while candidate.exists() {
        candidate = folder.join(format!("{} {}.canvas", untitled, counter));
        counter += 1;
    }

    let initial_content = r#"{
  "nodes": [],
  "edges": []
}"#;
    fs::write(&candidate, initial_content).map_err(|e| format!("Ошибка создания холста: {}", e))?;
    Ok(candidate)
}

/// Creates a new drawing note (.drawing.md) inside the specified folder.
pub fn create_drawing_in_folder(folder: &Path) -> Result<PathBuf, String> {
    if !folder.exists() {
        return Err(format!("Папка не существует: {:?}", folder));
    }

    let mut candidate = folder.join("Чертеж.drawing.md");
    let mut counter = 1;
    while candidate.exists() {
        candidate = folder.join(format!("Чертеж {}.drawing.md", counter));
        counter += 1;
    }

    let initial_content = "# Чертеж\n\n```excalidraw\n{}\n```\n";
    fs::write(&candidate, initial_content).map_err(|e| format!("Ошибка создания чертежа: {}", e))?;
    Ok(candidate)
}

/// Creates a Folder Note named after the folder inside itself (`<Folder>/<Folder>.md`).
pub fn create_folder_note(folder: &Path) -> Result<PathBuf, String> {
    if !folder.exists() {
        return Err(format!("Папка не существует: {:?}", folder));
    }

    let folder_name = folder
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Заметка папки".to_string());

    let candidate = folder.join(format!("{}.md", folder_name));
    if candidate.exists() {
        return Ok(candidate);
    }

    let initial_content = format!("# {}\n\nЗаметка папки.\n", folder_name);
    fs::write(&candidate, initial_content).map_err(|e| format!("Ошибка создания заметки папки: {}", e))?;
    Ok(candidate)
}

/// Duplicates the entire folder recursively.
pub fn duplicate_folder(folder: &Path) -> Result<PathBuf, String> {
    if !folder.exists() {
        return Err(format!("Папка не существует: {:?}", folder));
    }

    let parent = folder
        .parent()
        .ok_or_else(|| "Невозможно дублировать корневую директорию".to_string())?;

    let name = folder
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "папка".to_string());

    let mut candidate = parent.join(format!("{} (копия)", name));
    let mut counter = 2;
    while candidate.exists() {
        candidate = parent.join(format!("{} (копия {})", name, counter));
        counter += 1;
    }

    copy_dir_all(folder, &candidate)
        .map_err(|e| format!("Ошибка копирования папки: {}", e))?;

    Ok(candidate)
}

/// Renames a folder or file to a new name.
pub fn rename_folder(src: &Path, new_name: &str) -> Result<PathBuf, String> {
    if src.is_file() {
        return crate::views::sidebar::file_actions::rename_file(src, new_name);
    }
    let trimmed = new_name.trim();
    if trimmed.is_empty() {
        return Err("Имя папки не может быть пустым".to_string());
    }

    if trimmed.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']) {
        return Err("Имя папки содержит недопустимые символы".to_string());
    }

    let parent = src
        .parent()
        .ok_or_else(|| "Невозможно переименовать корневую директорию".to_string())?;

    let target = parent.join(trimmed);
    if target.exists() && target != src {
        return Err(format!("Папка или файл '{}' уже существует", trimmed));
    }

    fs::rename(src, &target).map_err(|e| format!("Ошибка переименования: {}", e))?;
    Ok(target)
}

/// Moves a folder into a destination folder.
pub fn move_folder(src: &Path, dest_parent: &Path) -> Result<PathBuf, String> {
    if !src.exists() {
        return Err("Исходная папка не существует".to_string());
    }
    if !dest_parent.exists() {
        return Err("Целевая папка не существует".to_string());
    }

    // Ensure dest_parent is not inside src (prevent circular nesting)
    if dest_parent.starts_with(src) {
        return Err("Нельзя переместить папку внутрь самой себя".to_string());
    }

    let name = src
        .file_name()
        .ok_or_else(|| "Некорректное имя папки".to_string())?;

    let target = dest_parent.join(name);
    if target.exists() {
        return Err(format!(
            "В целевой папке уже существует '{}'",
            target.file_name().unwrap_or_default().to_string_lossy()
        ));
    }

    fs::rename(src, &target).map_err(|e| format!("Ошибка перемещения папки: {}", e))?;
    Ok(target)
}

/// Deletes a folder and all its contents, or a single file if target is a file.
pub fn delete_folder_recursive(folder: &Path) -> Result<(), String> {
    if !folder.exists() {
        return Ok(());
    }
    if folder.is_dir() {
        fs::remove_dir_all(folder).map_err(|e| format!("Ошибка удаления папки: {}", e))
    } else {
        fs::remove_file(folder).map_err(|e| format!("Ошибка удаления файла: {}", e))
    }
}

/// Launches the native OS explorer window showing this folder.
pub fn show_in_explorer(path: &Path) {
    #[cfg(target_os = "windows")]
    {
        // Try opening explorer with folder focused
        let _ = std::process::Command::new("explorer")
            .arg(path)
            .spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .arg(path)
            .spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("xdg-open")
            .arg(path)
            .spawn();
    }
}

/// Computes the path relative to the workspace root.
pub fn get_relative_path_string(workspace: &Path, path: &Path) -> String {
    path.strip_prefix(workspace)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| path.to_string_lossy().to_string())
}

/// Scans workspace directories recursively, returning all folder paths except hidden ones.
pub fn collect_all_workspace_folders(workspace: &Path) -> Vec<PathBuf> {
    let mut folders = Vec::new();
    folders.push(workspace.to_path_buf());
    collect_folders_recursive(workspace, &mut folders);
    folders.sort();
    folders
}

fn collect_folders_recursive(dir: &Path, acc: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Ok(ft) = entry.file_type() {
                if ft.is_dir() {
                    let p = entry.path();
                    let name = entry.file_name().to_string_lossy().to_string();
                    if !name.starts_with('.') && name != "node_modules" && name != "target" {
                        acc.push(p.clone());
                        collect_folders_recursive(&p, acc);
                    }
                }
            }
        }
    }
}

/// Finds orphaned files (files in the folder that are not linked to by any note in the workspace).
pub fn find_orphaned_files_in_folder(workspace: &Path, folder: &Path) -> Vec<PathBuf> {
    let mut files_in_folder = Vec::new();
    if let Ok(entries) = fs::read_dir(folder) {
        for entry in entries.flatten() {
            if let Ok(ft) = entry.file_type() {
                if ft.is_file() {
                    let p = entry.path();
                    if p.extension().map(|e| e == "md").unwrap_or(false) {
                        files_in_folder.push(p);
                    }
                }
            }
        }
    }

    if files_in_folder.is_empty() {
        return Vec::new();
    }

    // Read all markdown notes in workspace to build link set
    let mut all_links = std::collections::HashSet::new();
    let mut all_ws_files = Vec::new();
    collect_md_files(workspace, &mut all_ws_files);

    for ws_file in all_ws_files {
        if let Ok(content) = fs::read_to_string(&ws_file) {
            // Very simple extraction of [[link]] and [text](link)
            for part in content.split("[[") {
                if let Some(end) = part.find("]]") {
                    let link = &part[..end];
                    let link_name = link.split('|').next().unwrap_or(link).trim();
                    all_links.insert(link_name.to_lowercase());
                }
            }
        }
    }

    // An orphaned file is one whose file stem is never referenced in any link
    files_in_folder
        .into_iter()
        .filter(|f| {
            if let Some(stem) = f.file_stem() {
                let stem_str = stem.to_string_lossy().to_lowercase();
                !all_links.contains(&stem_str)
            } else {
                false
            }
        })
        .collect()
}

fn collect_md_files(dir: &Path, acc: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Ok(ft) = entry.file_type() {
                let p = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') || name == "node_modules" || name == "target" {
                    continue;
                }
                if ft.is_dir() {
                    collect_md_files(&p, acc);
                } else if ft.is_file() && p.extension().map(|e| e == "md").unwrap_or(false) {
                    acc.push(p);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(PathBuf);
    impl TempDir {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!("vibecodium_test_{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&p).unwrap();
            Self(p)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn test_create_note_and_unique_naming() {
        let temp = TempDir::new();
        let folder = temp.path().join("sub");
        fs::create_dir_all(&folder).unwrap();

        let n1 = create_note_in_folder(&folder, "Без названия").unwrap();
        assert_eq!(n1.file_name().unwrap(), "Без названия.md");
        assert!(n1.exists());

        let n2 = create_note_in_folder(&folder, "Без названия").unwrap();
        assert_eq!(n2.file_name().unwrap(), "Без названия 1.md");
        assert!(n2.exists());
    }

    #[test]
    fn test_create_folder_and_canvas_and_drawing() {
        let temp = TempDir::new();
        let folder = temp.path().join("sub");
        fs::create_dir_all(&folder).unwrap();

        let f1 = create_folder_in_folder(&folder).unwrap();
        assert_eq!(f1.file_name().unwrap(), "Новая папка");
        assert!(f1.is_dir());

        let f2 = create_folder_in_folder(&folder).unwrap();
        assert_eq!(f2.file_name().unwrap(), "Новая папка 1");
        assert!(f2.is_dir());

        let c1 = create_canvas_in_folder(&folder, "Без названия").unwrap();
        assert_eq!(c1.file_name().unwrap(), "Без названия.canvas");
        assert!(c1.exists());

        let d1 = create_drawing_in_folder(&folder).unwrap();
        assert_eq!(d1.file_name().unwrap(), "Чертеж.drawing.md");
        assert!(d1.exists());
    }

    #[test]
    fn test_create_folder_note() {
        let temp = TempDir::new();
        let folder = temp.path().join("Projects");
        fs::create_dir_all(&folder).unwrap();

        let fnote = create_folder_note(&folder).unwrap();
        assert_eq!(fnote.file_name().unwrap(), "Projects.md");
        assert!(fnote.exists());
    }

    #[test]
    fn test_duplicate_folder() {
        let temp = TempDir::new();
        let folder = temp.path().join("Work");
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("note.md"), "hello").unwrap();

        let dup = duplicate_folder(&folder).unwrap();
        assert_eq!(dup.file_name().unwrap(), "Work (копия)");
        assert!(dup.join("note.md").exists());
    }

    #[test]
    fn test_rename_and_move_folder() {
        let temp = TempDir::new();
        let folder = temp.path().join("Alpha");
        fs::create_dir_all(&folder).unwrap();

        let renamed = rename_folder(&folder, "Beta").unwrap();
        assert_eq!(renamed.file_name().unwrap(), "Beta");
        assert!(renamed.exists());

        let dest = temp.path().join("Dest");
        fs::create_dir_all(&dest).unwrap();

        let moved = move_folder(&renamed, &dest).unwrap();
        assert_eq!(moved, dest.join("Beta"));
        assert!(moved.exists());
    }

    #[test]
    fn test_find_orphaned_files() {
        let temp = TempDir::new();
        let root = temp.path();
        let sub = root.join("Notes");
        fs::create_dir_all(&sub).unwrap();

        let file1 = sub.join("orphan.md");
        fs::write(&file1, "I have no links to me").unwrap();

        let file2 = sub.join("linked.md");
        fs::write(&file2, "I am linked").unwrap();

        let index_file = root.join("Index.md");
        fs::write(&index_file, "Link here: [[linked]]").unwrap();

        let orphaned = find_orphaned_files_in_folder(root, &sub);
        assert_eq!(orphaned.len(), 1);
        assert_eq!(orphaned[0], file1);
    }
}
