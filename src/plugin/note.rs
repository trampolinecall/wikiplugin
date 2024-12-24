use std::{
    cell::LazyCell,
    path::{Path, PathBuf},
    sync::LazyLock,
};

use nvim_rs::{compat::tokio::Compat, Buffer, Neovim};

use crate::{error::Error, plugin::Config};

pub struct Note {
    pub id: String,
}

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

    pub async fn scan_title(&self, config: &Config) -> Result<Option<String>, Error> {
        let contents = self.read_contents(config).await?;
        todo!()
    }

    pub async fn scan_tags(&self, config: &Config) -> Result<Vec<String>, Error> {
        let contents = self.read_contents(config).await?;
        todo!()
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
