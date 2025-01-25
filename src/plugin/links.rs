use std::path::{Path, PathBuf};

use pathdiff::diff_paths;

use crate::{
    error::Error,
    plugin::{
        note::{Note, PhysicalNote, ScratchNote},
        Config,
    },
};

pub fn format_link_path(config: &Config, current_note: &Note, target_file_path: &Path) -> Result<String, Error> {
    match current_note {
        Note::Physical(PhysicalNote { directories: _, id: _ }) => {
            let current_note_path = current_note.path(config).expect("physical note always has path");
            let current_file_parent_dir =
                current_note_path.parent().ok_or_else(|| format!("could not get parent of current file path {}", current_note_path.display()))?;
            let result = diff_paths(target_file_path, current_file_parent_dir)
                .ok_or_else(|| format!("could not construct link from {} to {}", current_note_path.display(), target_file_path.display()))?;
            Ok(result.to_str().ok_or_else(|| format!("could not convert link path to string: {}", result.display()))?.to_string())
        }
        Note::Scratch(ScratchNote { buffer: _ }) => Ok(target_file_path
            .to_str()
            .ok_or_else(|| format!("could not convert link target path to string: {}", target_file_path.display()))?
            .to_string()),
    }
}

pub fn resolve_link_path(config: &Config, current_note: &Note, link_path_text: &str) -> Result<PathBuf, Error> {
    let link_path = Path::new(link_path_text);
    match current_note {
        Note::Physical(PhysicalNote { directories: _, id: _ }) => {
            Ok(current_note.path(config).expect("physical note should always have a path").parent().ok_or("note path has no parent")?.join(link_path))
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
    fn format_link_abs_abs_test() {
        let current_path = Path::new("/absolute/to/wiki/dir/start.md");
        let target_path = Path::new("/absolute/to/wiki/dir/end.md");

        assert_eq!(format_link_path(Some(current_path), target_path).unwrap(), "end.md");
    }
    #[test] // TODO: this test fails
    fn format_link_rel_abs_test() {
        let current_path = Path::new("start.md"); // we assume that this is relative to the wiki home directory
        let target_path = Path::new("/absolute/to/wiki/dir/end.md");

        assert_eq!(format_link_path(Some(current_path), target_path).unwrap(), "end.md");
    }
    #[test]
    fn format_link_none_abs_test() {
        let target_path = Path::new("/absolute/to/wiki/dir/end.md");

        assert_eq!(format_link_path(None, target_path).unwrap(), "/absolute/to/wiki/dir/end.md");
    }
    #[test] // TODO: this test fails
    fn format_link_abs_rel_test() {
        let current_path = Path::new("/absolute/to/wiki/dir/start.md");
        let target_path = Path::new("end.md");

        assert_eq!(format_link_path(Some(current_path), target_path).unwrap(), "end.md");
    }
    #[test]
    fn format_link_rel_rel_test() {
        let current_path = Path::new("start.md");
        let target_path = Path::new("end.md");

        assert_eq!(format_link_path(Some(current_path), target_path).unwrap(), "end.md");
    }
    #[test]
    fn format_link_none_rel_test() {
        let target_path = Path::new("end.md");

        // TODO: decide whether or not this is the intended behavior (should assume that it is relative to wiki dir?)
        assert_eq!(format_link_path(None, target_path).unwrap(), "end.md");
    }
    #[test]
    fn format_link_target_more_nested_test() {
        let current_path = Path::new("/absolute/to/wiki/dir/start.md");
        let target_path = Path::new("/absolute/to/wiki/dir/dir2/end.md");

        assert_eq!(format_link_path(Some(current_path), target_path).unwrap(), "dir2/end.md");
    }
    #[test]
    fn format_link_target_same_directory_test() {
        let current_path = Path::new("/absolute/to/wiki/dir/dir2/start.md");
        let target_path = Path::new("/absolute/to/wiki/dir/dir2/end.md");

        assert_eq!(format_link_path(Some(current_path), target_path).unwrap(), "end.md");
    }
    #[test]
    fn format_link_target_less_nested_test() {
        let current_path = Path::new("/absolute/to/wiki/dir/dir2/start.md");
        let target_path = Path::new("/absolute/to/wiki/dir/end.md");

        assert_eq!(format_link_path(Some(current_path), target_path).unwrap(), "../end.md");
    }
}
