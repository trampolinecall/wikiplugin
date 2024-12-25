use std::{
    fmt::Display,
    path::{Path, PathBuf},
};

use markdown::{mdast::Node, to_mdast, Constructs, ParseOptions};
use nvim_rs::{compat::tokio::Compat, Buffer, Neovim};
use yaml_rust::Yaml;

use crate::{error::Error, plugin::Config};

#[derive(PartialEq, Eq)]
// TODO: make separate structs for physical and scratch notes?
pub enum Note {
    Physical { directories: Vec<String>, id: String },
    Scratch { buffer: Buffer<Compat<tokio::fs::File>> },
}
#[derive(Debug, PartialOrd, Ord, PartialEq, Eq, Clone)]
pub struct Tag(Vec<String>);

#[derive(Debug)]
struct MdParseError(markdown::message::Message);
impl std::fmt::Display for MdParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for MdParseError {}

impl Note {
    pub fn new_physical(directories: Vec<String>, id: String) -> Note {
        Note::Physical { directories, id }
    }

    pub async fn get_current_note(config: &Config, nvim: &mut Neovim<Compat<tokio::fs::File>>) -> Result<Note, Error> {
        let current_buf = nvim.get_current_buf().await?;
        let is_scratch = current_buf.get_option("buftype").await?.as_str().expect("option buftype should be a bool") == "nofile";
        if is_scratch {
            Ok(Note::Scratch { buffer: current_buf })
        } else {
            nvim_eval_and_cast!(current_buf_path_str, nvim, r#"expand("%:p")"#, as_str, "vim function expand( should always return a string");
            let path = Path::new(current_buf_path_str);
            Note::parse_from_filepath(config, path)
        }
    }

    // TODO: hide this function? replace it with get_current_buf_note?
    pub fn parse_from_filepath(config: &Config, path: &Path) -> Result<Note, Error> {
        let directories_path = if path.starts_with(&config.home_path) {
            path.strip_prefix(&config.home_path).expect("strip_prefix should return Ok if starts_with returns true")
        } else if !path.is_absolute() {
            path
        } else {
            Err("absolute path that does not point to a file within the wiki home directory is not a note")?
        };

        Ok(Note::Physical {
            directories: directories_path
                .parent()
                .ok_or("note path has no parent")?
                .iter()
                .map(|p| p.to_str().map(ToString::to_string))
                .collect::<Option<Vec<_>>>()
                .ok_or("note directories are not all valid strings")?,
            id: path.file_stem().ok_or("could not get file stem of note path")?.to_str().ok_or("os str is not valid str")?.to_string(),
        })
    }

    pub fn path(&self, config: &Config) -> Option<PathBuf> {
        match self {
            Note::Physical { directories, id } => {
                let mut path = config.home_path.clone();
                path.extend(directories);
                path.push(id);
                path.set_extension("md");
                Some(path)
            }
            Note::Scratch { buffer: _ } => None,
        }
    }

    pub async fn read_contents(&self, config: &Config, nvim: &mut Neovim<Compat<tokio::fs::File>>) -> Result<String, Error> {
        match self {
            Note::Physical { directories: _, id: _ } => {
                if let Some(buffer_contents) = self.read_contents_in_nvim(config, nvim).await? {
                    Ok(buffer_contents)
                } else {
                    Ok(tokio::fs::read_to_string(self.path(config).expect("physical note always has path")).await?)
                }
            }
            Note::Scratch { buffer } => Ok(buffer.get_lines(0, -1, false).await?.into_iter().map(|s| s + "\n").collect()),
        }
    }

    pub async fn parse_markdown(&self, config: &Config, nvim: &mut Neovim<Compat<tokio::fs::File>>) -> Result<Node, Error> {
        Ok(to_mdast(
            &self.read_contents(config, nvim).await?,
            &ParseOptions { constructs: Constructs { frontmatter: true, ..Constructs::gfm() }, ..ParseOptions::gfm() },
        )
        .map_err(MdParseError)?)
    }

    async fn find_frontmatter(&self, config: &Config, nvim: &mut Neovim<Compat<tokio::fs::File>>) -> Result<String, Error> {
        Ok(markdown_recursive_find_preorder(&self.parse_markdown(config, nvim).await?, &mut |node| match node {
            Node::Yaml(yaml) => Some(yaml.value.clone()),
            _ => None,
        })
        .ok_or("could not find frontmatter in file")?
        .1)
    }

    async fn parse_frontmatter(&self, config: &Config, nvim: &mut Neovim<Compat<tokio::fs::File>>) -> Result<Yaml, Error> {
        // TODO: swap_remove will panic if the yaml parser does not output any documents (i am not sure how that will happen though)
        Ok(yaml_rust::YamlLoader::load_from_str(&self.find_frontmatter(config, nvim).await?)?.swap_remove(0))
    }

    pub async fn scan_title(&self, config: &Config, nvim: &mut Neovim<Compat<tokio::fs::File>>) -> Result<String, Error> {
        Ok(self
            .parse_frontmatter(config, nvim)
            .await?
            .into_hash()
            .ok_or("frontmatter is not hash table at the top level")?
            .remove(&Yaml::String("title".to_string()))
            .ok_or("frontmatter has no title field")?
            .into_string()
            .ok_or("title is not string")?)
    }

    pub async fn scan_timestamp(&self, config: &Config, nvim: &mut Neovim<Compat<tokio::fs::File>>) -> Result<chrono::NaiveDateTime, Error> {
        let mut frontmatter = self.parse_frontmatter(config, nvim).await?.into_hash().ok_or("frontmatter is not hash table at the top level")?;
        let date = frontmatter
            .remove(&Yaml::String("date".to_string()))
            .ok_or("frontmatter has no date field")?
            .as_str()
            .ok_or("date field is not string")?
            .to_string();
        let time = frontmatter.remove(&Yaml::String("time".to_string()));

        let date = chrono::NaiveDate::parse_from_str(&date, &config.date_format)?;
        let time = match time {
            Some(time) => chrono::NaiveTime::parse_from_str(time.as_str().ok_or("time field is not string")?, &config.time_format)?,
            None => chrono::NaiveTime::MIN,
        };

        Ok(chrono::NaiveDateTime::new(date, time))
    }

    pub async fn scan_tags(&self, config: &Config, nvim: &mut Neovim<Compat<tokio::fs::File>>) -> Result<Vec<Tag>, Error> {
        let s = self
            .parse_frontmatter(config, nvim)
            .await?
            .into_hash()
            .ok_or("frontmatter is not hash table at the top level")?
            .remove(&Yaml::String("tags".to_string()))
            .ok_or("frontmatter has no tags field")?;
        match s {
            Yaml::String(s) => Ok(s.split(" ").map(Tag::parse_from_str).collect()),
            Yaml::Array(vec) => Ok(vec
                .into_iter()
                .map(|tag| Some(Tag::parse_from_str(tag.as_str()?)))
                .collect::<Option<Vec<_>>>()
                .ok_or("tags field is not array of strings")?),
            _ => Err("tags field is not string or array".to_string().into()),
        }
    }

    async fn get_buffer_in_nvim(
        &self,
        config: &Config,
        nvim: &mut Neovim<Compat<tokio::fs::File>>,
    ) -> Result<Option<nvim_rs::Buffer<Compat<tokio::fs::File>>>, Error> {
        match self {
            Note::Physical { directories: _, id: _ } => {
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
                    if buf_path == self.path(config).expect("physical note should always have path") {
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
            Note::Scratch { buffer } => Ok(Some(buffer.clone())),
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
            Note::Physical { directories: _, id } => Some(id),
            Note::Scratch { buffer: _ } => None,
        }
    }
}

impl Tag {
    fn parse_from_str(s: &str) -> Tag {
        Tag(s.split("::").map(ToString::to_string).collect())
    }
}
impl Display for Tag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.join("::"))
    }
}

// TODO: find a better place for these functions
pub fn markdown_recursive_find_preorder<'md, R>(node: &'md Node, pred: &mut impl FnMut(&Node) -> Option<R>) -> Option<(&'md Node, R)> {
    pred(node).map(|r| (node, r)).or_else(|| node.children().into_iter().flatten().find_map(|sn| markdown_recursive_find_preorder(sn, pred)))
}
pub fn markdown_recursive_find_postorder<'md, R>(node: &'md Node, pred: &mut impl FnMut(&Node) -> Option<R>) -> Option<(&'md Node, R)> {
    node.children().into_iter().flatten().find_map(|sn| markdown_recursive_find_postorder(sn, pred)).or_else(|| pred(node).map(|r| (node, r)))
}

pub fn point_in_position(position: &markdown::unist::Position, byte_index: usize) -> bool {
    byte_index >= position.start.offset && byte_index < position.end.offset
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

        let note_parsed = Note::parse_from_filepath(&config, Path::new("dir1/dir2/note.md")).expect("parse from filepath should work");
        assert_eq!(note_parsed, Note { directories: vec!["dir1".to_string(), "dir2".to_string()], id: "note".to_string() });
    }

    #[test]
    fn parse_from_filepath_absolute_in_home_test() {
        let config = Config {
            home_path: PathBuf::from("/path/to/wiki"),
            note_id_timestamp_format: String::new(),
            date_format: String::new(),
            time_format: String::new(),
        };

        let note_parsed = Note::parse_from_filepath(&config, Path::new("/path/to/wiki/dir1/dir2/note.md")).expect("parse from filepath should work");
        assert_eq!(note_parsed, Note { directories: vec!["dir1".to_string(), "dir2".to_string()], id: "note".to_string() });
    }

    #[test]
    fn parse_from_filepath_absolute_out_of_home_test() {
        let config = Config {
            home_path: PathBuf::from("/path/to/wiki"),
            note_id_timestamp_format: String::new(),
            date_format: String::new(),
            time_format: String::new(),
        };

        Note::parse_from_filepath(&config, Path::new("/some/other/directory/note.md")).expect_err("parse from filepath should not work in this case");
    }
}
