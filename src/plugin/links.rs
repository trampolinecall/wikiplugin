use std::{
    fmt::Display,
    path::{Path, PathBuf},
};

use pathdiff::diff_paths;

use crate::plugin::{
    note::{Note, PhysicalNote, ScratchNote},
    Config,
};

#[derive(Debug)]
pub enum FormatLinkPathError {
    TargetNotAbsolute,
    CurrentFilePathNoParent,
    CouldNotConstructLink,
    PathNotUtf8,
}
#[derive(Debug)]
pub enum ResolveLinkPathError {
    CurrentNoteNoParent,
}

impl Display for FormatLinkPathError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FormatLinkPathError::TargetNotAbsolute => write!(f, "target file path must be absolute because non-absolute target paths are ambiguous"),
            FormatLinkPathError::CurrentFilePathNoParent => write!(f, "could not get parent of current file path"),
            FormatLinkPathError::CouldNotConstructLink => write!(f, "could not construct link from"),
            FormatLinkPathError::PathNotUtf8 => write!(f, "link path is not valid unicode"),
        }
    }
}
impl Display for ResolveLinkPathError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolveLinkPathError::CurrentNoteNoParent => write!(f, "note path has no parent"),
        }
    }
}

pub fn format_link_path(config: &Config, current_note: &Note, target_file_path: &Path) -> Result<String, FormatLinkPathError> {
    if !(target_file_path.is_absolute()) {
        return Err(FormatLinkPathError::TargetNotAbsolute);
    }
    match current_note {
        Note::Physical(pn @ PhysicalNote { directories: _, id: _ }) => {
            let current_note_path = pn.path(config);
            let current_file_parent_dir = current_note_path.parent().ok_or(FormatLinkPathError::CurrentFilePathNoParent)?;
            let result = diff_paths(target_file_path, current_file_parent_dir).ok_or(FormatLinkPathError::CouldNotConstructLink)?;
            Ok(result.to_str().ok_or(FormatLinkPathError::PathNotUtf8)?.to_string())
        }
        Note::Scratch(ScratchNote { buffer: _ }) => Ok(target_file_path.to_str().ok_or(FormatLinkPathError::PathNotUtf8)?.to_string()),
    }
}

pub fn resolve_link_path(config: &Config, current_note: &Note, link_path_text: &str) -> Result<PathBuf, ResolveLinkPathError> {
    let link_path = Path::new(link_path_text);
    match current_note {
        Note::Physical(pn @ PhysicalNote { directories: _, id: _ }) => {
            Ok(pn.path(config).parent().ok_or(ResolveLinkPathError::CurrentNoteNoParent)?.join(link_path))
        }
        Note::Scratch(ScratchNote { buffer: _ }) => {
            // if this is a scratch buffer, there is no current path
            // so we open the target directory if it is absolute, and if not, make it absolute by prepending the config home directory
            if link_path.is_absolute() {
                Ok(link_path.to_path_buf())
            } else {
                Ok(config.home_path.join(link_path))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_link_path_to_abs_test() {
        let config = Config {
            home_path: PathBuf::from("/path/to/wiki"),
            note_id_timestamp_format: "%Y%m%d%H%M%S".to_string(),
            date_format: "%Y-%m-%d".to_string(),
            time_format: "%H:%M:%S".to_string(),
        };
        let current_note = Note::new_physical(vec![], "start".to_string());
        let target_note = &PathBuf::from("/path/to/wiki/end.md");

        assert_eq!(format_link_path(&config, &current_note, target_note).unwrap(), "end.md");
    }
    #[test]
    fn format_link_path_to_rel_test() {
        let config = Config {
            home_path: PathBuf::from("/path/to/wiki"),
            note_id_timestamp_format: "%Y%m%d%H%M%S".to_string(),
            date_format: "%Y-%m-%d".to_string(),
            time_format: "%H:%M:%S".to_string(),
        };
        let current_note = Note::new_physical(vec![], "start".to_string());
        let target_path = Path::new("end.md");

        format_link_path(&config, &current_note, target_path).unwrap_err();
    }

    #[test]
    fn format_link_target_more_nested_test() {
        let config = Config {
            home_path: PathBuf::from("/path/to/wiki"),
            note_id_timestamp_format: "%Y%m%d%H%M%S".to_string(),
            date_format: "%Y-%m-%d".to_string(),
            time_format: "%H:%M:%S".to_string(),
        };
        let current_note = Note::new_physical(vec!["dir".to_string()], "start".to_string());
        let target_path = Path::new("/path/to/wiki/dir/dir2/end.md");

        assert_eq!(format_link_path(&config, &current_note, target_path).unwrap(), "dir2/end.md");
    }
    #[test]
    fn format_link_target_same_directory_test() {
        let config = Config {
            home_path: PathBuf::from("/path/to/wiki"),
            note_id_timestamp_format: "%Y%m%d%H%M%S".to_string(),
            date_format: "%Y-%m-%d".to_string(),
            time_format: "%H:%M:%S".to_string(),
        };
        let current_note = Note::new_physical(vec!["dir".to_string(), "dir2".to_string()], "start".to_string());
        let target_path = Path::new("/path/to/wiki/dir/dir2/end.md");

        assert_eq!(format_link_path(&config, &current_note, target_path).unwrap(), "end.md");
    }
    #[test]
    fn format_link_target_less_nested_test() {
        let config = Config {
            home_path: PathBuf::from("/path/to/wiki"),
            note_id_timestamp_format: "%Y%m%d%H%M%S".to_string(),
            date_format: "%Y-%m-%d".to_string(),
            time_format: "%H:%M:%S".to_string(),
        };
        let current_note = Note::new_physical(vec!["dir".to_string(), "dir2".to_string()], "start".to_string());
        let target_path = Path::new("/path/to/wiki/dir/end.md");

        assert_eq!(format_link_path(&config, &current_note, target_path).unwrap(), "../end.md");
    }
}
