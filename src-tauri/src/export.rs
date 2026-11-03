//! Saving a rewrite as a Markdown file.
//!
//! The document itself is assembled in the frontend, where the mode label and the
//! rewrite already are; this side only decides where it goes and writes it. Downloads
//! is the destination because that is where a person looks for a file they just
//! exported — and because it needs no save dialog, no extra plugin, and no decision
//! the writer has to make about a folder.
//!
//! Nothing is ever overwritten: a second export in the same minute becomes `-2`.

use std::path::{Path, PathBuf};

/// Keeps a name to letters, digits, dashes and underscores, and ends it in `.md`.
///
/// The name comes from the frontend, which builds it from a mode and a timestamp, so
/// this is not so much trust as tidiness: a stray slash would silently write somewhere
/// else, and a `.md` twice is a name a text editor will show oddly.
pub fn safe_name(raw: &str) -> String {
    let mut cleaned = String::new();
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            cleaned.push(ch);
        } else if !cleaned.ends_with('-') {
            cleaned.push('-');
        }
    }
    let trimmed: String = cleaned.trim_matches('-').chars().take(64).collect();
    let stem = if trimmed.is_empty() { "quillnot".to_string() } else { trimmed };
    format!("{stem}.md")
}

/// The first free path in `dir`: the name itself, then `-2`, `-3`, and so on.
pub fn unique(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let stem = name.strip_suffix(".md").unwrap_or(name);
    for count in 2..1000 {
        let candidate = dir.join(format!("{stem}-{count}.md"));
        if !candidate.exists() {
            return candidate;
        }
    }
    dir.join(format!("{stem}-{}.md", std::process::id()))
}

/// `~/Downloads` when it exists, the home folder when it does not, and the temporary
/// directory when there is no home at all — in the last case the export still lands
/// somewhere, and the returned path says where.
pub fn downloads(home: Option<&Path>) -> PathBuf {
    match home {
        Some(home) if home.join("Downloads").is_dir() => home.join("Downloads"),
        Some(home) => home.to_path_buf(),
        None => std::env::temp_dir(),
    }
}

/// Writes a run to a Markdown file in the person's Downloads folder and answers with
/// the path, because that path is the only thing that tells them where it went.
#[tauri::command]
pub fn save_markdown(name: String, body: String) -> Result<String, String> {
    save(&name, &body).map(|path| path.display().to_string())
}

pub fn save(name: &str, body: &str) -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from);
    save_in(&downloads(home.as_deref()), name, body)
}

/// The part of `save` that can be pointed at a temporary directory and tested.
pub fn save_in(dir: &Path, name: &str, body: &str) -> Result<PathBuf, String> {
    let path = unique(dir, &safe_name(name));
    std::fs::write(&path, body)
        .map_err(|error| format!("Could not write {}: {error}", path.display()))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("quillnot-export-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    #[test]
    fn a_good_name_is_kept_and_given_an_extension() {
        assert_eq!(safe_name("quillnot-standard-20261103-1015"), "quillnot-standard-20261103-1015.md");
    }

    #[test]
    fn a_path_in_the_name_cannot_escape_the_folder() {
        let name = safe_name("../../.ssh/id_rsa");
        assert!(!name.contains('/'), "{name}");
        // The extension is the only dot left: no `..`, no hidden file.
        assert!(!name.trim_end_matches(".md").contains('.'), "{name}");
        assert_eq!(name, "ssh-id_rsa.md");
    }

    #[test]
    fn dots_are_not_kept_so_the_extension_is_the_only_one() {
        assert_eq!(safe_name("notes.v2"), "notes-v2.md");
    }

    #[test]
    fn an_empty_name_still_produces_a_file() {
        assert_eq!(safe_name(""), "quillnot.md");
        assert_eq!(safe_name("///"), "quillnot.md");
    }

    #[test]
    fn a_second_export_in_the_same_minute_does_not_overwrite_the_first() {
        let dir = scratch("unique");
        let first = unique(&dir, "run.md");
        assert_eq!(first.file_name().unwrap(), "run.md");
        fs::write(&first, "one").unwrap();

        let second = unique(&dir, "run.md");
        assert_eq!(second.file_name().unwrap(), "run-2.md");
        fs::write(&second, "two").unwrap();

        assert_eq!(fs::read_to_string(&first).unwrap(), "one");
        assert_eq!(unique(&dir, "run.md").file_name().unwrap(), "run-3.md");
    }

    #[test]
    fn downloads_is_used_when_it_exists_and_home_when_it_does_not() {
        let home = scratch("home");
        assert_eq!(downloads(Some(&home)), home);

        fs::create_dir_all(home.join("Downloads")).unwrap();
        assert_eq!(downloads(Some(&home)), home.join("Downloads"));

        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn saving_writes_the_document_and_keeps_both_copies() {
        let dir = scratch("save");
        let body = "# Rewrite\n\nThe text.\n";

        let first = save_in(&dir, "quillnot-standard-20261103-1015", body).expect("first save");
        assert_eq!(first.parent(), Some(dir.as_path()));
        assert_eq!(fs::read_to_string(&first).unwrap(), body);

        let second = save_in(&dir, "quillnot-standard-20261103-1015", "second").expect("second save");
        assert_ne!(first, second);
        assert_eq!(fs::read_to_string(&first).unwrap(), body);
        assert_eq!(fs::read_to_string(&second).unwrap(), "second");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn without_a_home_the_export_still_has_somewhere_to_go() {
        assert_eq!(downloads(None), std::env::temp_dir());
    }
}
