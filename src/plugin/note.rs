use std::{
    fmt::Display,
    path::{Path, PathBuf},
};

use anyhow::Error;
use nvim_rs::{compat::tokio::Compat, Buffer, Neovim};

use crate::plugin::Config;

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone)]
pub struct PhysicalNote {
    pub directories: Vec<String>,
    pub id: String,
}

#[derive(PartialEq, Eq, Clone)]
pub struct ScratchNote {
    pub buffer: Buffer<Compat<tokio::fs::File>>,
}

#[derive(PartialEq, Eq, Clone)]
pub enum Note {
    Physical(PhysicalNote),
    Scratch(ScratchNote),
}
#[derive(Debug, PartialOrd, Ord, PartialEq, Eq, Clone)]
pub struct Tag(Vec<String>);

impl PhysicalNote {
    pub fn parse_from_filepath(config: &Config, path: &Path) -> Result<PhysicalNote, Error> {
        let path_abs_canon = if path.is_absolute() { path.canonicalize()? } else { config.home_path.join(path).canonicalize()? };
        let directories_path = if path_abs_canon.starts_with(&config.home_path) {
            path_abs_canon.strip_prefix(&config.home_path).expect("strip_prefix should return Ok if starts_with returns true")
        } else {
            Err(Error::msg("path that does not point to a file within the wiki home directory is not a note"))?
        };

        Ok(PhysicalNote {
            directories: directories_path
                .parent()
                .ok_or(Error::msg("note path has no parent"))?
                .iter()
                .map(|p| p.to_str().map(ToString::to_string))
                .collect::<Option<Vec<_>>>()
                .ok_or(Error::msg("note directories are not all valid strings"))?,
            id: path
                .file_stem()
                .ok_or(Error::msg("could not get file stem of note path"))?
                .to_str()
                .ok_or(Error::msg("os str is not valid str"))?
                .to_string(),
        })
    }

    pub fn path(&self, config: &Config) -> PathBuf {
        let mut path = config.home_path.clone();
        path.extend(&self.directories);
        path.push(&self.id);
        path.set_extension("md");
        path
    }

    pub async fn read_contents(&self, config: &Config, nvim: &mut Neovim<Compat<tokio::fs::File>>) -> Result<String, Error> {
        log::info!("reading contents of file {}", self.path(config).display());
        if let Some(buffer_contents) = self.read_contents_in_nvim(config, nvim).await? {
            Ok(buffer_contents)
        } else {
            Ok(tokio::fs::read_to_string(self.path(config)).await?)
        }
    }

    async fn get_buffer_in_nvim(
        &self,
        config: &Config,
        nvim: &mut Neovim<Compat<tokio::fs::File>>,
    ) -> Result<Option<nvim_rs::Buffer<Compat<tokio::fs::File>>>, Error> {
        let buflist = nvim.list_bufs().await?;
        let mut current_buf = None;
        for buf in buflist {
            let buf_number = &buf.get_number().await?;
            nvim_eval_and_cast!(
                buf_path,
                nvim,
                &format!(r##"expand("#{}:p")"##, buf_number),
                as_str,
                "vim function expand( should always return a number"
            );
            let buf_path = Path::new(buf_path);
            if buf_path == self.path(config) {
                current_buf = Some(buf);
                break;
            }
        }

        match current_buf {
            Some(b) => {
                if b.is_loaded().await? {
                    Ok(Some(b))
                } else {
                    Ok(None)
                }
            }
            None => Ok(None),
        }
    }
    // TODO: this function is duplicated verbatim with Note
    async fn read_contents_in_nvim(&self, config: &Config, nvim: &mut Neovim<Compat<tokio::fs::File>>) -> Result<Option<String>, Error> {
        match self.get_buffer_in_nvim(config, nvim).await? {
            Some(buf) => Ok(Some(buf.get_lines(0, -1, false).await?.into_iter().map(|s| s + "\n").collect())),
            None => Ok(None),
        }
    }
}
impl Note {
    pub fn new_physical(directories: Vec<String>, id: String) -> Note {
        Note::Physical(PhysicalNote { directories, id })
    }

    pub async fn get_current_note(config: &Config, nvim: &mut Neovim<Compat<tokio::fs::File>>) -> Result<Note, Error> {
        let current_buf = nvim.get_current_buf().await?;
        let is_scratch = current_buf.get_option("buftype").await?.as_str().expect("option buftype should be a bool") == "nofile";
        if is_scratch {
            Ok(Note::Scratch(ScratchNote { buffer: current_buf }))
        } else {
            nvim_eval_and_cast!(current_buf_path_str, nvim, r#"expand("%:p")"#, as_str, "vim function expand( should always return a string");
            let path = Path::new(current_buf_path_str);
            Ok(Note::Physical(PhysicalNote::parse_from_filepath(config, path)?))
        }
    }

    pub fn path(&self, config: &Config) -> Option<PathBuf> {
        match self {
            Note::Physical(n) => Some(n.path(config)),
            Note::Scratch(ScratchNote { buffer: _ }) => None,
        }
    }

    pub async fn read_contents(&self, config: &Config, nvim: &mut Neovim<Compat<tokio::fs::File>>) -> Result<String, Error> {
        match self {
            Note::Physical(n) => n.read_contents(config, nvim).await,
            Note::Scratch(ScratchNote { buffer }) => Ok(buffer.get_lines(0, -1, false).await?.into_iter().map(|s| s + "\n").collect()),
        }
    }

    async fn get_buffer_in_nvim(
        &self,
        config: &Config,
        nvim: &mut Neovim<Compat<tokio::fs::File>>,
    ) -> Result<Option<nvim_rs::Buffer<Compat<tokio::fs::File>>>, Error> {
        match self {
            Note::Physical(n) => n.get_buffer_in_nvim(config, nvim).await,
            Note::Scratch(ScratchNote { buffer }) => Ok(Some(buffer.clone())),
        }
    }
    async fn read_contents_in_nvim(&self, config: &Config, nvim: &mut Neovim<Compat<tokio::fs::File>>) -> Result<Option<String>, Error> {
        match self.get_buffer_in_nvim(config, nvim).await? {
            Some(buf) => Ok(Some(buf.get_lines(0, -1, false).await?.into_iter().map(|s| s + "\n").collect())),
            None => Ok(None),
        }
    }

    /// Returns `true` if the note is [`Physical`].
    ///
    /// [`Physical`]: Note::Physical
    #[must_use]
    pub fn is_physical(&self) -> bool {
        matches!(self, Self::Physical { .. })
    }

    /// Returns `true` if the note is [`Scratch`].
    ///
    /// [`Scratch`]: Note::Scratch
    #[must_use]
    pub fn is_scratch(&self) -> bool {
        matches!(self, Self::Scratch { .. })
    }

    pub fn get_id(&self) -> Option<&str> {
        match self {
            Note::Physical(PhysicalNote { directories: _, id }) => Some(id),
            Note::Scratch(ScratchNote { buffer: _ }) => None,
        }
    }

    pub fn as_physical(&self) -> Option<&PhysicalNote> {
        if let Self::Physical(v) = self {
            Some(v)
        } else {
            None
        }
    }

    pub fn as_scratch(&self) -> Option<&ScratchNote> {
        if let Self::Scratch(v) = self {
            Some(v)
        } else {
            None
        }
    }
}

impl Tag {
    pub fn parse_from_str(s: &str) -> Tag {
        Tag(s.split("::").map(ToString::to_string).collect())
    }
}
impl Display for Tag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.join("::"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_from_filepath_relative_test() {
        let config = Config {
            home_path: PathBuf::from("/path/to/wiki"),
            note_id_timestamp_format: String::new(),
            date_format: String::new(),
            time_format: String::new(),
        };

        let note_parsed = PhysicalNote::parse_from_filepath(&config, Path::new("dir1/dir2/note.md")).expect("parse from filepath should work");
        assert_eq!(note_parsed, PhysicalNote { directories: vec!["dir1".to_string(), "dir2".to_string()], id: "note".to_string() });
    }

    #[test]
    fn parse_from_filepath_absolute_in_home_test() {
        let config = Config {
            home_path: PathBuf::from("/path/to/wiki"),
            note_id_timestamp_format: String::new(),
            date_format: String::new(),
            time_format: String::new(),
        };

        let note_parsed =
            PhysicalNote::parse_from_filepath(&config, Path::new("/path/to/wiki/dir1/dir2/note.md")).expect("parse from filepath should work");
        assert_eq!(note_parsed, PhysicalNote { directories: vec!["dir1".to_string(), "dir2".to_string()], id: "note".to_string() });
    }

    #[test]
    fn parse_from_filepath_absolute_out_of_home_test() {
        let config = Config {
            home_path: PathBuf::from("/path/to/wiki"),
            note_id_timestamp_format: String::new(),
            date_format: String::new(),
            time_format: String::new(),
        };

        PhysicalNote::parse_from_filepath(&config, Path::new("/some/other/directory/note.md"))
            .expect_err("parse from filepath should not work in this case");
    }
}
