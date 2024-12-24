use std::path::{Path, PathBuf};

use markdown_it::MarkdownIt;
use nvim_rs::{compat::tokio::Compat, Neovim};
use pathdiff::diff_paths;

use crate::{connection, error::Error, plugin::note::Note};

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

pub enum Message {
    NewNote { directory: String, focus: bool },
    OpenIndex, // TODO: configurable index file name?
    DeleteNote,
    NewNoteAndInsertLink,
    Invalid(String),
}

impl Message {
    #[inline]
    pub fn parse(method: String, args: Vec<nvim_rs::Value>) -> Message {
        fn to_string(method_name: &str, argument_name: &str, v: &nvim_rs::Value) -> Result<String, String> {
            v.as_str().ok_or_else(move || format!("argument '{argument_name}' of method '{method_name}' is not a string")).map(|s| s.to_string())
        }
        fn to_bool(method_name: &str, argument_name: &str, v: &nvim_rs::Value) -> Result<bool, String> {
            v.as_bool().ok_or_else(move || format!("argument '{argument_name}' of method '{method_name}' is not a bool"))
        }

        macro_rules! parse_params {
            ([$(($pname:ident, $parse_function:expr)),*], $method_name:ident, $params:ident, || $result:expr) => {
                {
                    // we need the "asdf" so that if the method accepts no parameters, the array's type can be inferred to be an array of &'static str
                    // (we cannot use a type annotation of [&'static str; _] because that is unstable: see rust issue #85077)
                    let num_params = ["asdf" as &'static str, $(stringify!($pname)),*].len() - 1;
                    if $params.len() == num_params {
                        #[allow(unused_assignments, unused_variables, unused_mut, clippy::redundant_closure_call)]

                        let result = (|| {
                        let mut param_index = 0;
                            $(
                                let $pname = $parse_function(&$method_name, stringify!($pname), &$params[param_index])?;
                                param_index += 1;
                            )*

                            Ok::<_, String>($result)
                        })();

                        match result {
                            Ok(res) => res,
                            Err(err) => Message::Invalid(err),
                        }
                    } else {
                        Message::Invalid(format!("method '{}' needs {} parameters", $method_name, num_params))
                    }
                }
            };
        }

        match &*method {
            "new_note" => parse_params!([(directory, to_string), (focus, to_bool)], method, args, || Message::NewNote { directory, focus }),
            "open_index" => parse_params!([], method, args, || Message::OpenIndex),
            "new_note_and_insert_link" => parse_params!([], method, args, || Message::NewNoteAndInsertLink),
            "delete_note" => parse_params!([], method, args, || Message::DeleteNote),
            _ => Message::Invalid(format!("unknown method '{method}' with params {args:?}")),
        }
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
            Message::NewNote { directory, focus } => self.new_note(&mut nvim, directory, focus).await.map(|_| ()),
            Message::OpenIndex => self.open_index(&mut nvim).await,
            Message::NewNoteAndInsertLink => self.new_note_and_insert_link(&mut nvim).await,
            Message::DeleteNote => self.delete_note(&mut nvim).await,
            Message::Invalid(e) => Err(e.into()),
        };

        if let Err(e) = result {
            connection::print_error(&mut nvim, e).await;
        }
    }
}

impl WikiPlugin {
    async fn new_note(&self, nvim: &mut Neovim<Compat<tokio::fs::File>>, directory: String, focus: bool) -> Result<Note, Error> {
        let title = nvim.eval(r#"input("note name: ")"#).await?;
        let title = title.as_str().expect("vim function input( should always return a string");

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

        Ok(Note::new(note_id, Some(buf)))
    }

    async fn open_index(&self, nvim: &mut Neovim<Compat<tokio::fs::File>>) -> Result<(), Error> {
        let index_path = self.config.home_path.join("index.md");
        let index_path: &str = index_path.to_str().ok_or_else(|| format!("invalid note index path {index_path:?}"))?;
        nvim.cmd(vec![("cmd".into(), "edit".into()), ("args".into(), vec![nvim_rs::Value::from(index_path)].into())], vec![]).await?;

        Ok(())
    }

    async fn new_note_and_insert_link(&self, nvim: &mut Neovim<Compat<tokio::fs::File>>) -> Result<(), Error> {
        let new_note = self.new_note(nvim, "".to_string(), false).await?;
        self.insert_link_at_cursor(nvim, &new_note, None).await?;
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
            None => link_to.scan_title(&self.config).await?.unwrap_or(String::new()),
        };

        let current_buf_path_str = nvim.eval(r#"expand("%:p")"#).await?;
        let current_buf_path = Path::new(current_buf_path_str.as_str().expect("vim function expand( should always return a string"));
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
        let current_buf_path_str = nvim.eval(r#"expand("%:p")"#).await?;
        let current_buf_path = Path::new(current_buf_path_str.as_str().expect("vim function expand( should always return a string"));

        let choice =
            nvim.eval(r#"input("are you sure you want to delete this note?\noptions: 'yes' for yes, anything else for no\ninput: ")"#).await?;
        let choice = choice.as_str().expect("vim function input( should always return a string");
        if choice == "yes" {
            nvim.command(&format!(r#"echo "\n{} deleted""#, current_buf_path.to_str().expect("current buffer path should be utf8"))).await?;
            std::fs::remove_file(current_buf_path)?;
        } else {
            nvim.command(r#"echo "\nnot deleting""#).await?;
        }
        Ok(())
    }

    async fn list_all_files(&self, nvim: &mut Neovim<Compat<tokio::fs::File>>) -> Result<Vec<String>, Error> {
        let things =
            nvim.eval(&format!("glob({}/**/*.md)", self.config.home_path.to_str().expect("wiki home path should always be valid unicode"))).await?;
        let paths: Vec<_> = things
            .as_array()
            .expect("vim fn glob( should return an array")
            .iter()
            .map(|path| path.as_str().expect("vim fn glob( array elements should be strings").to_string())
            .collect();
        Ok(paths)
    }
}
