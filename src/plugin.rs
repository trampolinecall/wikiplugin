use std::path::{Path, PathBuf};

use nvim_rs::{compat::tokio::Compat, Neovim};
use pathdiff::diff_paths;

use crate::{
    connection,
    error::Error,
    plugin::{messages::Message, note::Note},
};

mod messages;
mod note;

#[derive(Clone)]
pub struct Config {
    home_path: PathBuf,
    note_id_timestamp_format: String,
    date_format: String,
    time_format: String,
}
impl Config {
    pub fn parse_from_args() -> Config {
        let mut args = std::env::args().skip(1);
        let c = Config {
            home_path: args.next().expect("first argument should be wiki home path").into(),
            note_id_timestamp_format: args.next().expect("second argument should be note id timestamp format"),
            date_format: args.next().expect("third argument should be date format"),
            time_format: args.next().expect("fourth argument should be time format"),
        };
        assert_eq!(args.next(), None, "there should only be 4 arguments");
        c
    }
}

#[derive(Clone)]
pub struct WikiPlugin {
    pub config: Config,
}

#[async_trait::async_trait]
impl nvim_rs::Handler for WikiPlugin {
    type Writer = Compat<tokio::fs::File>;

    async fn handle_notify(&self, name: String, args: Vec<nvim_rs::Value>, mut nvim: Neovim<Compat<tokio::fs::File>>) {
        let message = Message::parse(name, args);

        let result = match message {
            Message::NewNote { directory, focus } => self.new_note(&mut nvim, &directory, focus).await.map(|_| ()),
            Message::OpenIndex {} => self.open_index(&mut nvim).await,
            Message::NewNoteAndInsertLink {} => self.new_note_and_insert_link(&mut nvim).await,
            Message::InsertLinkAtCursor { link_to_id, link_text } => self.insert_link_at_cursor(&mut nvim, &Note::new(link_to_id), link_text).await,
            Message::InsertLinkAtCursorOrCreate { link_to_id, link_text } => {
                let n;
                let note = match link_to_id {
                    Some(link_to_id) => {
                        n = Note::new(link_to_id);
                        Some(&n)
                    }
                    None => None,
                };

                self.insert_link_at_cursor_or_create(&mut nvim, note, link_text).await
            }
            Message::DeleteNote {} => self.delete_note(&mut nvim).await,
            Message::Invalid(e) => Err(e.into()),
        };

        if let Err(e) = result {
            connection::print_error(&mut nvim, e).await;
        }
    }
}

macro_rules! nvim_eval_and_cast {
    ($vname:ident, $nvim:expr, $eval:expr, $cast:ident, $expect_message:expr) => {
        let $vname = $nvim.eval($eval).await?;
        let $vname = $vname.$cast().expect($expect_message);
    };
}
impl WikiPlugin {
    async fn new_note(&self, nvim: &mut Neovim<Compat<tokio::fs::File>>, directory: &str, focus: bool) -> Result<Note, Error> {
        nvim_eval_and_cast!(title, nvim, r#"input("note name: ")"#, as_str, "vim function input( should always return a string");

        let now = chrono::Local::now();
        let note_id = now.format(&self.config.note_id_timestamp_format).to_string();

        let buf_path = {
            let mut p = self.config.home_path.join(directory);
            p.push(&note_id);
            p.set_extension("md");
            p
        };

        // TODO: customizable templates?
        let buf_contents = [
            "---".to_string(),
            format!("title: {title}"),
            format!("date: {}", now.format(&self.config.date_format)),
            format!("time: {}", now.format(&self.config.time_format)),
            "tags:".to_string(),
            "---".to_string(),
        ]
        .to_vec();

        let buf = nvim.create_buf(true, false).await?;
        buf.set_name(buf_path.to_str().ok_or_else(|| format!("invalid buf path {buf_path:?}"))?).await?;
        buf.set_lines(0, 0, true, buf_contents).await?;
        buf.set_option("filetype", "zet".into()).await?; // TODO: filetype not zet

        if focus {
            nvim.set_current_buf(&buf).await?;
        }

        Ok(Note::new(note_id))
    }

    async fn open_index(&self, nvim: &mut Neovim<Compat<tokio::fs::File>>) -> Result<(), Error> {
        let index_path = self.config.home_path.join("index.md");
        let index_path: &str = index_path.to_str().ok_or_else(|| format!("invalid note index path {index_path:?}"))?;
        nvim.cmd(vec![("cmd".into(), "edit".into()), ("args".into(), vec![nvim_rs::Value::from(index_path)].into())], vec![]).await?;

        Ok(())
    }

    async fn new_note_and_insert_link(&self, nvim: &mut Neovim<Compat<tokio::fs::File>>) -> Result<(), Error> {
        let new_note = self.new_note(nvim, "", false).await?;
        self.insert_link_at_cursor(nvim, &new_note, None).await?;
        Ok(())
    }

    async fn insert_link_at_cursor_or_create(
        &self,
        nvim: &mut Neovim<Compat<tokio::fs::File>>,
        link_to: Option<&Note>,
        link_text: Option<String>,
    ) -> Result<(), Error> {
        let note = match link_to {
            Some(link_to) => link_to,
            None => &self.new_note(nvim, "", false).await?,
        };
        self.insert_link_at_cursor(nvim, note, link_text).await?;
        Ok(())
    }

    async fn insert_link_at_cursor(
        &self,
        nvim: &mut Neovim<Compat<tokio::fs::File>>,
        link_to: &Note,
        link_text: Option<String>,
    ) -> Result<(), Error> {
        let link_text = match link_text {
            Some(lt) => lt,
            None => link_to.scan_title(&self.config).await.unwrap_or(String::new()),
        };

        nvim_eval_and_cast!(current_buf_path_str, nvim, r#"expand("%:p")"#, as_str, "vim function expand( should always return a string");
        let current_buf_path = Path::new(current_buf_path_str);
        let current_buf_parent_dir = current_buf_path
            .parent()
            .ok_or_else(|| format!("could not get parent of current buffer because current buffer path is {current_buf_path:?}"))?;

        let link_path = diff_paths(link_to.path(&self.config), current_buf_parent_dir)
            .ok_or_else(|| format!("could not construct link path to link from {:?} to {:?}", current_buf_parent_dir, link_to.path(&self.config)))?;
        let link_path = link_path.to_str().ok_or_else(|| format!("could not convert link path to str: {link_path:?}"))?;

        nvim.put(vec![format!("[{link_text}]({link_path})")], "c", false, true).await?;

        Ok(())
    }

    async fn follow_link(&self, nvim: &mut Neovim<Compat<tokio::fs::File>>) -> Result<(), Error> {
        todo!()
    }

    async fn delete_note(&self, nvim: &mut Neovim<Compat<tokio::fs::File>>) -> Result<(), Error> {
        nvim_eval_and_cast!(current_buf_path_str, nvim, r#"expand("%:p")"#, as_str, "vim function expand( should always return a string");
        let current_buf_path = Path::new(current_buf_path_str);

        nvim_eval_and_cast!(
            choice,
            nvim,
            r#"input("are you sure you want to delete this note?\noptions: 'yes' for yes, anything else for no\ninput: ")"#,
            as_str,
            "vim function input( should always return a string"
        );
        if choice == "yes" {
            std::fs::remove_file(current_buf_path)?;
            nvim.command(&format!(r#"echo "\n{} deleted""#, current_buf_path.to_str().expect("current buffer path should be utf8"))).await?;
        } else {
            nvim.command(r#"echo "\nnot deleting""#).await?;
        }
        Ok(())
    }

    async fn list_all_files(&self, nvim: &mut Neovim<Compat<tokio::fs::File>>) -> Result<Vec<String>, Error> {
        nvim_eval_and_cast!(
            things,
            nvim,
            &format!("glob({}/**/*.md)", self.config.home_path.to_str().expect("wiki home path should always be valid unicode")),
            as_array,
            "vim fn glob( should return an array"
        );
        let paths: Vec<_> = things.iter().map(|path| path.as_str().expect("vim fn glob( array elements should be strings").to_string()).collect();
        Ok(paths)
    }
}
