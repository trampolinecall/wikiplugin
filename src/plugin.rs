use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use nvim_rs::{compat::tokio::Compat, Neovim};
use pathdiff::diff_paths;

use crate::{
    connection,
    error::Error,
    plugin::{
        messages::Message,
        note::{Note, Tag},
    },
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
            Message::DeleteNote {} => self.delete_note(&mut nvim).await,
            Message::NewNoteAndInsertLink {} => self.new_note_and_insert_link(&mut nvim).await,
            Message::OpenTagIndex {} => self.open_tag_index(&mut nvim).await,
            Message::FollowLink {} => self.follow_link(&mut nvim).await,
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
            Message::Invalid(e) => Err(e.into()),
        };

        if let Err(e) = result {
            connection::print_error(&mut nvim, e).await;
        }
    }
}

macro_rules! nvim_eval_and_cast {
    ($vname:ident, $nvim:expr, $eval:expr, $cast:ident, $error_message:expr) => {
        let $vname = $nvim.eval($eval).await?;
        let $vname = $vname.$cast().ok_or($error_message)?;
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
        buf.set_option("filetype", "wikipluginnote".into()).await?;

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

    async fn open_tag_index(&self, nvim: &mut Neovim<Compat<tokio::fs::File>>) -> Result<(), Error> {
        let notes = self.list_all_notes()?;
        let mut tag_table: BTreeMap<Tag, Vec<(&Note, String, PathBuf)>> = BTreeMap::new(); // TODO: eventually this should become &(Note, String, PathBuf)
        let mut tag_list = BTreeSet::new();

        for note in &notes {
            log::debug!("{}", note.id);
            let title = note.scan_title(&self.config).await.unwrap_or("poop".to_string()); // TODO: this is not a real solution
            let tags = note.scan_tags(&self.config).await.unwrap_or(Vec::new());
            let path = note.path(&self.config);

            for tag in tags {
                tag_table.entry(tag.clone()).or_default().push((note, title.clone(), path.clone()));
                tag_list.insert(tag);
            }
        }

        let buffer = nvim.create_buf(true, true).await?;
        buffer.set_option("filetype", "wikipluginnote".into()).await?;

        for tag in tag_list {
            buffer.set_lines(-2, -2, false, vec![format!("# {}", tag), "".to_string()]).await?;

            for (_, note_title, note_path) in &tag_table[&tag] {
                buffer
                    .set_lines(-2, -2, false, vec![format!("- [{}]({})", note_title, note_path.to_str().ok_or("PathBuf contains invalid unicode")?)])
                    .await?;
            }

            buffer.set_lines(-2, -2, false, vec!["".to_string()]).await?;
        }

        nvim.set_current_buf(&buffer).await?;

        Ok(())
    }

    async fn follow_link(&self, nvim: &mut Neovim<Compat<tokio::fs::File>>) -> Result<(), Error> {
        nvim_eval_and_cast!(current_note_id, nvim, r#"expand("%:t:r")"#, as_str, "vim function expand( should always return a string");

        let note = Note::new(current_note_id.to_string());
        let md = note.parse_markdown(&self.config).await?;

        nvim_eval_and_cast!(cursor_byte_index, nvim, r#"line2byte(line(".")) + col(".") - 1 - 1"#, as_u64, "byte index should be a number");
        let (_, link_path) = note::markdown_recursive_find_preorder(&md, &mut |node| match node {
            markdown::mdast::Node::Link(markdown::mdast::Link { children: _, position: Some(position), url, title: _ }) => {
                log::debug!("{:?}, byte index is {}", position, cursor_byte_index);
                if note::point_in_position(position, cursor_byte_index.try_into().expect("byte index u64 does not fit into usize")) {
                    Some(url.to_string())
                } else {
                    None
                }
            }
            _ => None,
        })
        .ok_or("not on a link")?;

        let new_note_path = note.path(&self.config).parent().ok_or("note path has no parent")?.join(PathBuf::from(link_path));

        nvim.cmd(
            vec![
                ("cmd".into(), "edit".into()),
                ("args".into(), vec![nvim_rs::Value::from(new_note_path.to_str().ok_or("pathbuf cannot be converted to string")?)].into()),
            ],
            vec![],
        )
        .await?;

        Ok(())
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
            nvim.command(&format!(r#"echo "\n{} deleted""#, current_buf_path.to_str().ok_or("current buffer path should be utf8")?)).await?;
        } else {
            nvim.command(r#"echo "\nnot deleting""#).await?;
        }
        Ok(())
    }

    fn list_all_notes(&self) -> Result<Vec<Note>, Error> {
        glob::glob(&format!("{}/**/*.md", self.config.home_path.to_str().ok_or("wiki home path should always be valid unicode")?))?
            .map(|path| match path {
                Ok(path) => Ok(Note {
                    id: path.as_path().file_stem().ok_or("glob returned invalid path")?.to_str().ok_or("os str is not valid str")?.to_string(),
                }),
                Err(e) => Err(e)?,
            })
            .collect::<Result<Vec<_>, _>>()
    }
}
