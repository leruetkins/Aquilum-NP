use std::path::{Path, PathBuf};
use std::fs;

/// Duplicates a file in the same directory (`<name> (copy).md`, etc.).
pub fn duplicate_file(file: &Path) -> Result<PathBuf, String> {
    if !file.exists() {
        return Err(format!("Файл не существует: {:?}", file));
    }

    let parent = file
        .parent()
        .ok_or_else(|| "Невозможно дублировать файл в корне".to_string())?;

    let stem = file
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "файл".to_string());

    let ext = file
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();

    let mut candidate = parent.join(format!("{} (копия){}", stem, ext));
    let mut counter = 2;
    while candidate.exists() {
        candidate = parent.join(format!("{} (копия {}){}", stem, counter, ext));
        counter += 1;
    }

    fs::copy(file, &candidate).map_err(|e| format!("Ошибка копирования файла: {}", e))?;
    Ok(candidate)
}

/// Renames a file to a new name.
pub fn rename_file(src: &Path, new_name: &str) -> Result<PathBuf, String> {
    let trimmed = new_name.trim();
    if trimmed.is_empty() {
        return Err("Имя файла не может быть пустым".to_string());
    }

    if trimmed.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']) {
        return Err("Имя файла содержит недопустимые символы".to_string());
    }

    let parent = src
        .parent()
        .ok_or_else(|| "Невозможно переименовать файл без родительской папки".to_string())?;

    let ext = src
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();

    // Preserve original extension if user did not include it
    let final_name = if !ext.is_empty() && !trimmed.ends_with(&ext) {
        format!("{}{}", trimmed, ext)
    } else {
        trimmed.to_string()
    };

    let target = parent.join(&final_name);
    if target.exists() && target != src {
        return Err(format!("Файл '{}' уже существует", final_name));
    }
    // Renaming to the same name is a no-op, not a filesystem call: the inline
    // rename edits the stem only, so committing it unchanged lands here.
    if target.as_path() == src {
        return Ok(target);
    }

    fs::rename(src, &target).map_err(|e| format!("Ошибка переименования: {}", e))?;
    Ok(target)
}

/// Moves a file to another folder.
pub fn move_file(src: &Path, dest_folder: &Path) -> Result<PathBuf, String> {
    if !src.exists() {
        return Err("Исходный файл не существует".to_string());
    }
    if !dest_folder.exists() {
        return Err("Целевая папка не существует".to_string());
    }

    let file_name = src
        .file_name()
        .ok_or_else(|| "Некорректное имя файла".to_string())?;

    let target = dest_folder.join(file_name);
    if target.exists() {
        return Err(format!(
            "В целевой папке уже существует файл '{}'",
            target.file_name().unwrap_or_default().to_string_lossy()
        ));
    }

    fs::rename(src, &target).map_err(|e| format!("Ошибка перемещения файла: {}", e))?;
    Ok(target)
}

/// Deletes a file.
pub fn delete_file(file: &Path) -> Result<(), String> {
    if !file.exists() {
        return Ok(());
    }
    fs::remove_file(file).map_err(|e| format!("Ошибка удаления файла: {}", e))
}

/// Opens a file in the OS default application.
pub fn open_in_default_app(path: &Path) {
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/c", "start", "", &path.to_string_lossy()])
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

/// Simple percent encoding for URL query parameters.
fn percent_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(b as char);
            }
            _ => {
                out.push_str(&format!("%{:02X}", b));
            }
        }
    }
    out
}

/// Formats the Obsidian URI for the given file: `obsidian://open?vault=<vault>&file=<rel_path>`.
pub fn get_obsidian_uri(workspace: &Path, file: &Path) -> String {
    let vault_name = workspace
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "vault".to_string());

    let rel = file
        .strip_prefix(workspace)
        .map(|p| p.to_string_lossy().to_string().replace('\\', "/"))
        .unwrap_or_else(|_| file.to_string_lossy().to_string().replace('\\', "/"));

    let encoded_vault = percent_encode(&vault_name);
    let encoded_file = percent_encode(&rel);

    format!("obsidian://open?vault={}&file={}", encoded_vault, encoded_file)
}
