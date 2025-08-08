use std::{
    fmt::Display,
    path::{Path, PathBuf},
};

use nvim_oxi::api::{self, Buffer};

use crate::plugin::Config;

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone)]
pub struct PhysicalNote {
    pub directories: Vec<String>,
    pub id: String,
}

#[derive(PartialEq, Eq, Clone)]
pub struct ScratchNote {
    pub buffer: Buffer,
}

#[derive(PartialEq, Eq, Clone)]
pub enum Note {
    Physical(PhysicalNote),
    Scratch(ScratchNote),
}
#[derive(Debug, PartialOrd, Ord, PartialEq, Eq, Clone)]
pub struct Tag(Vec<String>);

#[derive(Debug)]
pub enum ParseFromFilepathError {
    CannotCanonicalize(std::io::Error),
    FileNotWithinWikiDir,
    NoFileStem,
    NoPathParent,
    OsStringNotValidString,
}
impl Display for ParseFromFilepathError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseFromFilepathError::CannotCanonicalize(error) => write!(f, "cannot canonicalize path: {error}"),
            ParseFromFilepathError::FileNotWithinWikiDir => write!(f, "file is not within wiki directory"),
            ParseFromFilepathError::NoFileStem => write!(f, "file does not have stem"),
            ParseFromFilepathError::NoPathParent => write!(f, "path does not have parent"),
            ParseFromFilepathError::OsStringNotValidString => write!(f, "os strings are not valid strings"),
        }
    }
}

error_union! {
    pub enum ReadContentsError {
        Io(std::io::Error),
        NvimApi(api::Error),
    }
}

error_union! {
    pub enum GetCurrentNoteError {
        NvimApi(api::Error),
        ParseFromFilepathError(ParseFromFilepathError),
    }
}

impl PhysicalNote {
    pub fn parse_from_filepath(config: &Config, path: &Path) -> Result<PhysicalNote, ParseFromFilepathError> {
        let path_abs_canon = if path.is_absolute() {
            path.canonicalize().map_err(ParseFromFilepathError::CannotCanonicalize)?
        } else {
            config.home_path.join(path).canonicalize().map_err(ParseFromFilepathError::CannotCanonicalize)?
        };
        let directories_path = if path_abs_canon.starts_with(&config.home_path) {
            path_abs_canon.strip_prefix(&config.home_path).expect("strip_prefix should return Ok if starts_with returns true")
        } else {
            Err(ParseFromFilepathError::FileNotWithinWikiDir)?
        };

        Ok(PhysicalNote {
            directories: directories_path
                .parent()
                .ok_or(ParseFromFilepathError::NoPathParent)?
                .iter()
                .map(|p| p.to_str().map(ToString::to_string))
                .collect::<Option<Vec<_>>>()
                .ok_or(ParseFromFilepathError::OsStringNotValidString)?,
            id: path
                .file_stem()
                .ok_or(ParseFromFilepathError::NoFileStem)?
                .to_str()
                .ok_or(ParseFromFilepathError::OsStringNotValidString)?
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

    pub fn read_contents(&self, config: &Config) -> Result<String, ReadContentsError> {
        log::info!("reading contents of file {}", self.path(config).display());
        if let Some(buffer_contents) = self.read_contents_in_nvim(config)? {
            Ok(buffer_contents)
        } else {
            Ok(std::fs::read_to_string(self.path(config))?)
        }
    }

    fn get_buffer_in_nvim(&self, config: &Config) -> Result<Option<Buffer>, api::Error> {
        let buflist = api::list_bufs();
        let mut current_buf = None;
        for buf in buflist {
            let buf_number = &buf.handle();
            let buf_path: String = nvim_oxi::api::eval(&format!(r##"expand("#{buf_number}:p")"##))?;
            let buf_path = Path::new(&buf_path);
            if buf_path == self.path(config) {
                current_buf = Some(buf);
                break;
            }
        }

        match current_buf {
            Some(b) => {
                if b.is_loaded() {
                    Ok(Some(b))
                } else {
                    Ok(None)
                }
            }
            None => Ok(None),
        }
    }
    // TODO: this function is duplicated verbatim with Note
    fn read_contents_in_nvim(&self, config: &Config) -> Result<Option<String>, api::Error> {
        match self.get_buffer_in_nvim(config)? {
            Some(buf) => Ok(Some(buf.get_lines(.., false)?.map(|s| s.to_string_lossy().to_string() + "\n").collect())), // TODO: find a better solution than to_string_lossy
            None => Ok(None),
        }
    }
}
impl Note {
    pub fn new_physical(directories: Vec<String>, id: String) -> Note {
        Note::Physical(PhysicalNote { directories, id })
    }

    pub fn get_current_note(config: &Config) -> Result<Note, GetCurrentNoteError> {
        let current_buf = nvim_oxi::api::get_current_buf();
        let is_scratch =
            nvim_oxi::api::get_option_value::<String>("buftype", &nvim_oxi::api::opts::OptionOpts::builder().buffer(current_buf.clone()).build())?
                == "nofile";
        if is_scratch {
            Ok(Note::Scratch(ScratchNote { buffer: current_buf }))
        } else {
            let current_buf_path_str: String = nvim_oxi::api::eval(r#"expand("%:p")"#)?;
            let path = Path::new(&current_buf_path_str);
            Ok(Note::Physical(PhysicalNote::parse_from_filepath(config, path)?))
        }
    }

    pub fn path(&self, config: &Config) -> Option<PathBuf> {
        match self {
            Note::Physical(n) => Some(n.path(config)),
            Note::Scratch(ScratchNote { buffer: _ }) => None,
        }
    }

    pub fn read_contents(&self, config: &Config) -> Result<String, ReadContentsError> {
        match self {
            Note::Physical(n) => n.read_contents(config),
            Note::Scratch(ScratchNote { buffer }) => Ok(buffer.get_lines(.., false)?.map(|s| s.to_string_lossy().to_string() + "\n").collect()), // TODO: find a better solution than to_string_lossy
        }
    }

    fn get_buffer_in_nvim(&self, config: &Config) -> Result<Option<Buffer>, api::Error> {
        match self {
            Note::Physical(n) => n.get_buffer_in_nvim(config),
            Note::Scratch(ScratchNote { buffer }) => Ok(Some(buffer.clone())),
        }
    }
    fn read_contents_in_nvim(&self, config: &Config) -> Result<Option<String>, api::Error> {
        match self.get_buffer_in_nvim(config)? {
            Some(buf) => Ok(Some(buf.get_lines(.., false)?.map(|s| s.to_string_lossy().to_string() + "\n").collect())), // TODO: find a better solution than to_string_lossy
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
