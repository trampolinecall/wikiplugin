use std::path::{Path, PathBuf};

use markdown::{mdast::Node, to_mdast, Constructs, ParseOptions};
use nvim_rs::{compat::tokio::Compat, Neovim};
use yaml_rust::Yaml;

use crate::{error::Error, plugin::Config};

pub struct Note {
    pub id: String,
}

#[derive(Debug)]
struct MdParseError(markdown::message::Message);
impl std::fmt::Display for MdParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for MdParseError {}

impl Note {
    pub fn new(id: String) -> Note {
        Note { id }
    }

    pub fn path(&self, config: &Config) -> PathBuf {
        config.home_path.join(&self.id).with_extension("md")
    }

    pub async fn read_contents(&self, config: &Config) -> Result<String, Error> {
        tokio::fs::read_to_string(self.path(config)).await.map_err(Into::into)
    }

    // TODO: sometimes this produces counterintuitive results (especially when being used to find
    // the node under the cursor position) because it is always reading the markdown as it appears
    // on the disk and not as it appears in the vim buffer (which might be modified and not written yet)
    pub async fn parse_markdown(&self, config: &Config) -> Result<Node, Error> {
        Ok(to_mdast(
            &self.read_contents(config).await?,
            &ParseOptions { constructs: Constructs { frontmatter: true, ..Constructs::gfm() }, ..ParseOptions::gfm() },
        )
        .map_err(MdParseError)?)
    }

    async fn find_frontmatter(&self, config: &Config) -> Result<String, Error> {
        Ok(markdown_recursive_find_preorder(&self.parse_markdown(config).await?, &mut |node| match node {
            Node::Yaml(yaml) => Some(yaml.value.clone()),
            _ => None,
        })
        .ok_or("could not find frontmatter in file")?
        .1)
    }

    async fn parse_frontmatter(&self, config: &Config) -> Result<Yaml, Error> {
        // TODO: swap_remove will panic if the yaml parser does not output any documents (i am not sure how that will happen though)
        Ok(yaml_rust::YamlLoader::load_from_str(&self.find_frontmatter(config).await?)?.swap_remove(0))
    }

    pub async fn scan_title(&self, config: &Config) -> Result<String, Error> {
        Ok(self
            .parse_frontmatter(config)
            .await?
            .into_hash()
            .ok_or("frontmatter is not hash table at the top level")?
            .remove(&Yaml::String("title".to_string()))
            .ok_or("frontmatter has no title field")?
            .into_string()
            .ok_or("title is not string")?)
    }

    pub async fn scan_tags(&self, config: &Config) -> Result<Vec<String>, Error> {
        let s = self
            .parse_frontmatter(config)
            .await?
            .into_hash()
            .ok_or("frontmatter is not hash table at the top level")?
            .remove(&Yaml::String("tags".to_string()))
            .ok_or("frontmatter has no tags field")?;
        match s {
            Yaml::String(s) => Ok(s.split(" ").map(ToString::to_string).collect()),
            Yaml::Array(vec) => {
                Ok(vec.into_iter().map(|tag| tag.into_string()).collect::<Option<Vec<_>>>().ok_or("tags field is not array of strings")?)
            }
            _ => Err("tags field is not string or array".into()),
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
            let buf_name = &buf.get_name().await?;
            let buf_path = Path::new(buf_name);
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
    async fn get_contents_in_nvim(&self, config: &Config, nvim: &mut Neovim<Compat<tokio::fs::File>>) -> Result<Option<String>, Error> {
        match self.get_buffer_in_nvim(config, nvim).await? {
            Some(buf) => {
                let lines = buf.get_lines(0, -1, false).await?;
                Ok(Some(lines.join("\n")))
            }
            None => Ok(None),
        }
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
