use std::path::{Path, PathBuf};

use nvim_rs::{compat::tokio::Compat, Neovim};

use crate::{error::Error, plugin::Config};

struct Note {
    id: String,
}

impl Note {
    fn new(id: String) -> Note {
        Note { id }
    }

    fn path(&self, config: &Config) -> PathBuf {
        config.home_path.join(&self.id).with_extension("md")
    }

    async fn read_contents(&self, config: &Config) -> Result<String, Error> {
        tokio::fs::read_to_string(self.path(config)).await.map_err(Into::into)
    }

    async fn get_buffer_in_nvim(&self, config: &Config, nvim: &mut Neovim<Compat<tokio::fs::File>>) -> Result<Option<nvim_rs::Buffer<Compat<tokio::fs::File>>>, Error> {
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

