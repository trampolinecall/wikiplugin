use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use nvim_rs::{compat::tokio::Compat, Neovim};
use regex::Regex;

use crate::{
    connection::{self, ConnectionError},
    plugin::{
        messages::{NotifyMessage, RequestMessage},
        note::{Note, PhysicalNote, Tag},
    },
};

// TODO: find a better place for this
macro_rules! nvim_eval_and_cast {
    ($vname:ident, $nvim:expr, $eval:expr, $cast:ident, $error_message:expr) => {
        let $vname = $nvim.eval($eval).await?;
        let $vname = $vname.$cast().ok_or($crate::connection::ConnectionError::InvalidLuaToRustCast($error_message))?;
    };
}

mod links;
mod markdown;
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
        let home_path: PathBuf = args.next().expect("first argument should be wiki home path").into();
        if !home_path.is_absolute() {
            panic!("home path should be absolute");
        }
        let c = Config {
            home_path,
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

    async fn handle_request(
        &self,
        name: String,
        args: Vec<nvim_rs::Value>,
        mut nvim: Neovim<Compat<tokio::fs::File>>,
    ) -> Result<nvim_rs::Value, nvim_rs::Value> {
        let message = RequestMessage::parse(name, args);

        #[derive(Debug)]
        enum Error {
            AutogenerateError(AutogenerateError),
            MessageParseError(messages::MessageParseError),
        }
        impl std::fmt::Display for Error {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self {
                    Error::AutogenerateError(e) => e.fmt(f),
                    Error::MessageParseError(e) => e.fmt(f),
                }
            }
        }
        impl std::error::Error for Error {}

        let result: Result<nvim_rs::Value, Error> = match message {
            RequestMessage::RegenerateAutogeneratedSections {} => {
                self.regenerate_autogenerated_sections(&mut nvim).await.map(|()| nvim_rs::Value::Nil).map_err(Error::AutogenerateError)
            }
            RequestMessage::Invalid(e) => Err(Error::MessageParseError(e)),
        };

        match result {
            Ok(v) => Ok(v),
            Err(e) => {
                connection::print_error(&mut nvim, &e).await;
                Err(nvim_rs::Value::Nil)
            }
        }
    }

    async fn handle_notify(&self, name: String, args: Vec<nvim_rs::Value>, mut nvim: Neovim<Compat<tokio::fs::File>>) {
        let message = NotifyMessage::parse(name, args);

        #[derive(Debug)]
        enum Error {
            Connection(ConnectionError),
            NonUtf8Path,
            DeleteNote(DeleteNoteError),
            InsertLink(InsertLinkError),
            FollowLink(FollowLinkError),
            TagIndex(TagIndexError),
            MessageParse(messages::MessageParseError),
        }
        impl From<ConnectionErrorOrNonUtf8Path> for Error {
            fn from(v: ConnectionErrorOrNonUtf8Path) -> Self {
                match v {
                    ConnectionErrorOrNonUtf8Path::ConnectionError(connection_error) => Error::Connection(connection_error),
                    ConnectionErrorOrNonUtf8Path::NonUtf8Path => Error::NonUtf8Path,
                }
            }
        }
        impl std::fmt::Display for Error {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self {
                    Error::Connection(e) => e.fmt(f),
                    Error::NonUtf8Path => write!(f, "path is not valid utf 8"),
                    Error::InsertLink(e) => e.fmt(f),
                    Error::MessageParse(e) => e.fmt(f),
                    Error::TagIndex(e) => e.fmt(f),
                    Error::DeleteNote(e) => e.fmt(f),
                    Error::FollowLink(e) => e.fmt(f),
                }
            }
        }
        impl std::error::Error for Error {}

        let result: Result<(), Error> = match message {
            NotifyMessage::NewNote { directory, focus } => self.new_note(&mut nvim, directory, focus).await.map(|_| ()).map_err(Into::into),
            NotifyMessage::OpenIndex {} => self.open_index(&mut nvim).await.map_err(Into::into),
            NotifyMessage::DeleteNote {} => self.delete_note(&mut nvim).await.map_err(Error::DeleteNote),
            NotifyMessage::NewNoteAndInsertLink {} => self.new_note_and_insert_link(&mut nvim).await.map_err(Error::InsertLink),
            NotifyMessage::OpenTagIndex {} => self.open_tag_index(&mut nvim).await.map_err(Error::TagIndex),
            NotifyMessage::FollowLink {} => self.follow_link(&mut nvim).await.map_err(Error::FollowLink),
            NotifyMessage::InsertLinkAtCursor { link_to_directories, link_to_id, link_text } => {
                // TODO: move this logic somewhere else
                self.insert_link_at_cursor(&mut nvim, &Note::new_physical(link_to_directories, link_to_id), link_text)
                    .await
                    .map_err(Error::InsertLink)
            }
            NotifyMessage::InsertLinkAtCursorOrCreate { link_to_directories, link_to_id, link_text } => {
                let n;
                let note = match link_to_id {
                    Some(link_to_id) => {
                        n = Note::new_physical(link_to_directories, link_to_id); // TODO: move this logic somewhere else
                        Some(&n)
                    }
                    None => None,
                };

                self.insert_link_at_cursor_or_create(&mut nvim, note, link_text).await.map_err(Error::InsertLink)
            }
            NotifyMessage::InsertLinkToPathAtCursorOrCreate { link_to_path, link_text } => {
                self.insert_link_to_path_at_cursor_or_create(&mut nvim, link_to_path, link_text).await.map_err(Error::InsertLink)
            }

            NotifyMessage::Invalid(e) => Err(Error::MessageParse(e)),
        };

        if let Err(e) = result {
            connection::print_error(&mut nvim, &e).await;
        }
    }
}

enum ConnectionErrorOrNonUtf8Path {
    ConnectionError(ConnectionError),
    NonUtf8Path,
}
impl<T: Into<ConnectionError>> From<T> for ConnectionErrorOrNonUtf8Path {
    fn from(v: T) -> Self {
        Self::ConnectionError(v.into())
    }
}

#[derive(Debug)]
enum InsertLinkError {
    ParseFromFilepathError(note::ParseFromFilepathError),
    FormatLinkPathError(links::FormatLinkPathError),
    ConnectionError(ConnectionError),
    NonUtf8Path,
    ParseMarkdownError(markdown::MdParseError), // TODO: remove these? if the frontmatter or title is incorrect just put nothing
    InvalidFrontmatter(markdown::InvalidFrontmatter),
    GetTitleError(markdown::GetFrontmatterFieldError),
    CannotLinkToScratchNote,
}
impl std::fmt::Display for InsertLinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InsertLinkError::ParseFromFilepathError(e) => e.fmt(f),
            InsertLinkError::ConnectionError(e) => e.fmt(f),
            InsertLinkError::NonUtf8Path => write!(f, "path is not valid utf 8"),
            InsertLinkError::FormatLinkPathError(e) => e.fmt(f),
            InsertLinkError::GetTitleError(e) => e.fmt(f),
            InsertLinkError::ParseMarkdownError(e) => e.fmt(f),
            InsertLinkError::InvalidFrontmatter(e) => e.fmt(f),
            InsertLinkError::CannotLinkToScratchNote => write!(f, "cannot link to scratch note"),
        }
    }
}
impl From<Box<nvim_rs::error::CallError>> for InsertLinkError {
    fn from(v: Box<nvim_rs::error::CallError>) -> Self {
        Self::ConnectionError(ConnectionError::CallError(v))
    }
}
impl From<markdown::InvalidFrontmatter> for InsertLinkError {
    fn from(v: markdown::InvalidFrontmatter) -> Self {
        Self::InvalidFrontmatter(v)
    }
}
impl From<markdown::GetFrontmatterFieldError> for InsertLinkError {
    fn from(v: markdown::GetFrontmatterFieldError) -> Self {
        Self::GetTitleError(v)
    }
}
impl From<markdown::MdParseError> for InsertLinkError {
    fn from(v: markdown::MdParseError) -> Self {
        Self::ParseMarkdownError(v)
    }
}
impl From<links::FormatLinkPathError> for InsertLinkError {
    fn from(v: links::FormatLinkPathError) -> Self {
        Self::FormatLinkPathError(v)
    }
}
impl From<ConnectionError> for InsertLinkError {
    fn from(v: ConnectionError) -> Self {
        Self::ConnectionError(v)
    }
}
impl From<ConnectionErrorOrNonUtf8Path> for InsertLinkError {
    fn from(v: ConnectionErrorOrNonUtf8Path) -> Self {
        match v {
            ConnectionErrorOrNonUtf8Path::ConnectionError(connection_error) => InsertLinkError::ConnectionError(connection_error),
            ConnectionErrorOrNonUtf8Path::NonUtf8Path => InsertLinkError::NonUtf8Path,
        }
    }
}
impl From<note::ParseFromFilepathError> for InsertLinkError {
    fn from(v: note::ParseFromFilepathError) -> Self {
        Self::ParseFromFilepathError(v)
    }
}

#[derive(Debug)]
enum TagIndexError {
    ListAllPhysicalNotesError(ListAllPhysicalNotesError),
    ConnectionError(ConnectionError),
    NonUtf8Path,
    ParseMarkdownError(markdown::MdParseError), // TODO: remove these? if the frontmatter or title is incorrect just put nothing
    InvalidFrontmatter(markdown::InvalidFrontmatter),
}
impl std::fmt::Display for TagIndexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TagIndexError::ListAllPhysicalNotesError(e) => e.fmt(f),
            TagIndexError::ConnectionError(e) => e.fmt(f),
            TagIndexError::NonUtf8Path => write!(f, "path is not valid utf 8"),
            TagIndexError::ParseMarkdownError(e) => e.fmt(f),
            TagIndexError::InvalidFrontmatter(e) => e.fmt(f),
        }
    }
}
impl From<Box<nvim_rs::error::CallError>> for TagIndexError {
    fn from(v: Box<nvim_rs::error::CallError>) -> Self {
        Self::ConnectionError(ConnectionError::CallError(v))
    }
}
impl From<ListAllPhysicalNotesError> for TagIndexError {
    fn from(v: ListAllPhysicalNotesError) -> Self {
        Self::ListAllPhysicalNotesError(v)
    }
}
impl From<markdown::InvalidFrontmatter> for TagIndexError {
    fn from(v: markdown::InvalidFrontmatter) -> Self {
        Self::InvalidFrontmatter(v)
    }
}
impl From<markdown::MdParseError> for TagIndexError {
    fn from(v: markdown::MdParseError) -> Self {
        Self::ParseMarkdownError(v)
    }
}
impl From<ConnectionError> for TagIndexError {
    fn from(v: ConnectionError) -> Self {
        Self::ConnectionError(v)
    }
}

#[derive(Debug)]
enum FollowLinkError {
    ConnectionError(ConnectionError),
    ParseFromFilepathError(note::ParseFromFilepathError),
    ParseMarkdownError(markdown::MdParseError),
    NotOnALink,
    ResolveLinkPathError(links::ResolveLinkPathError),
    NonUtf8Path,
}
impl std::fmt::Display for FollowLinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FollowLinkError::ConnectionError(e) => e.fmt(f),
            FollowLinkError::ParseFromFilepathError(e) => e.fmt(f),
            FollowLinkError::ParseMarkdownError(e) => e.fmt(f),
            FollowLinkError::NotOnALink => write!(f, "not on a link"),
            FollowLinkError::ResolveLinkPathError(e) => e.fmt(f),
            FollowLinkError::NonUtf8Path => write!(f, "path is not valid utf8"),
        }
    }
}

impl From<links::ResolveLinkPathError> for FollowLinkError {
    fn from(v: links::ResolveLinkPathError) -> Self {
        Self::ResolveLinkPathError(v)
    }
}
impl From<Box<nvim_rs::error::CallError>> for FollowLinkError {
    fn from(v: Box<nvim_rs::error::CallError>) -> Self {
        Self::ConnectionError(ConnectionError::CallError(v))
    }
}
impl From<markdown::MdParseError> for FollowLinkError {
    fn from(v: markdown::MdParseError) -> Self {
        Self::ParseMarkdownError(v)
    }
}
impl From<note::ParseFromFilepathError> for FollowLinkError {
    fn from(v: note::ParseFromFilepathError) -> Self {
        Self::ParseFromFilepathError(v)
    }
}
impl From<ConnectionError> for FollowLinkError {
    fn from(v: ConnectionError) -> Self {
        Self::ConnectionError(v)
    }
}

#[derive(Debug)]
enum DeleteNoteError {
    ConnectionError(ConnectionError),
    NonUtf8CurrentBufferPath,
    IoError(std::io::Error),
}
impl std::fmt::Display for DeleteNoteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeleteNoteError::ConnectionError(e) => e.fmt(f),
            DeleteNoteError::NonUtf8CurrentBufferPath => write!(f, "current buffer path is not valid utf8"),
            DeleteNoteError::IoError(e) => e.fmt(f),
        }
    }
}
impl From<std::io::Error> for DeleteNoteError {
    fn from(v: std::io::Error) -> Self {
        Self::IoError(v)
    }
}
impl From<Box<nvim_rs::error::CallError>> for DeleteNoteError {
    fn from(v: Box<nvim_rs::error::CallError>) -> Self {
        Self::ConnectionError(ConnectionError::CallError(v))
    }
}
impl From<ConnectionError> for DeleteNoteError {
    fn from(v: ConnectionError) -> Self {
        Self::ConnectionError(v)
    }
}

#[derive(Debug)]
enum AutogenerateError {
    ConnectionError(ConnectionError),
    ListAllPhysicalNotesError(ListAllPhysicalNotesError),
    MdParseError(markdown::MdParseError), // TODO: remove most of these errors and just dont list files that trigger them?
    InvalidFrontmatter(markdown::InvalidFrontmatter),
    GetFrontmatterFieldError(markdown::GetFrontmatterFieldError),
    GetTimestampError(markdown::GetTimestampError),
    FormatLinkPathError(links::FormatLinkPathError),
    ResolveLinkPathError(links::ResolveLinkPathError),
    ParseFromFilepathError(note::ParseFromFilepathError),
}
impl std::fmt::Display for AutogenerateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AutogenerateError::ConnectionError(e) => e.fmt(f),
            AutogenerateError::ListAllPhysicalNotesError(e) => e.fmt(f),
            AutogenerateError::MdParseError(e) => e.fmt(f),
            AutogenerateError::InvalidFrontmatter(e) => e.fmt(f),
            AutogenerateError::GetFrontmatterFieldError(e) => e.fmt(f),
            AutogenerateError::GetTimestampError(e) => e.fmt(f),
            AutogenerateError::FormatLinkPathError(e) => e.fmt(f),
            AutogenerateError::ResolveLinkPathError(e) => e.fmt(f),
            AutogenerateError::ParseFromFilepathError(e) => e.fmt(f),
        }
    }
}
impl From<links::ResolveLinkPathError> for AutogenerateError {
    fn from(v: links::ResolveLinkPathError) -> Self {
        Self::ResolveLinkPathError(v)
    }
}
impl From<note::ParseFromFilepathError> for AutogenerateError {
    fn from(v: note::ParseFromFilepathError) -> Self {
        Self::ParseFromFilepathError(v)
    }
}
impl From<links::FormatLinkPathError> for AutogenerateError {
    fn from(v: links::FormatLinkPathError) -> Self {
        Self::FormatLinkPathError(v)
    }
}
impl From<markdown::GetTimestampError> for AutogenerateError {
    fn from(v: markdown::GetTimestampError) -> Self {
        Self::GetTimestampError(v)
    }
}
impl From<markdown::GetFrontmatterFieldError> for AutogenerateError {
    fn from(v: markdown::GetFrontmatterFieldError) -> Self {
        Self::GetFrontmatterFieldError(v)
    }
}
impl From<markdown::InvalidFrontmatter> for AutogenerateError {
    fn from(v: markdown::InvalidFrontmatter) -> Self {
        Self::InvalidFrontmatter(v)
    }
}
impl From<markdown::MdParseError> for AutogenerateError {
    fn from(v: markdown::MdParseError) -> Self {
        Self::MdParseError(v)
    }
}
impl From<ListAllPhysicalNotesError> for AutogenerateError {
    fn from(v: ListAllPhysicalNotesError) -> Self {
        Self::ListAllPhysicalNotesError(v)
    }
}
impl From<Box<nvim_rs::error::CallError>> for AutogenerateError {
    fn from(v: Box<nvim_rs::error::CallError>) -> Self {
        Self::ConnectionError(ConnectionError::CallError(v))
    }
}
impl From<ConnectionError> for AutogenerateError {
    fn from(v: ConnectionError) -> Self {
        Self::ConnectionError(v)
    }
}

#[derive(Debug)]
enum ListAllPhysicalNotesError {
    NonUtf8HomePath,
    GlobPatternError(glob::PatternError),
    GlobError(glob::GlobError),
    ParseFromFilepathError(note::ParseFromFilepathError),
}
impl std::fmt::Display for ListAllPhysicalNotesError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ListAllPhysicalNotesError::NonUtf8HomePath => write!(f, "home path is not valid utf8"),
            ListAllPhysicalNotesError::GlobPatternError(e) => e.fmt(f),
            ListAllPhysicalNotesError::GlobError(e) => e.fmt(f),
            ListAllPhysicalNotesError::ParseFromFilepathError(e) => e.fmt(f),
        }
    }
}
impl From<note::ParseFromFilepathError> for ListAllPhysicalNotesError {
    fn from(v: note::ParseFromFilepathError) -> Self {
        Self::ParseFromFilepathError(v)
    }
}
impl From<glob::PatternError> for ListAllPhysicalNotesError {
    fn from(v: glob::PatternError) -> Self {
        Self::GlobPatternError(v)
    }
}
impl From<glob::GlobError> for ListAllPhysicalNotesError {
    fn from(v: glob::GlobError) -> Self {
        Self::GlobError(v)
    }
}
impl WikiPlugin {
    async fn new_note(
        &self,
        nvim: &mut Neovim<Compat<tokio::fs::File>>,
        directories: Vec<String>,
        focus: bool,
    ) -> Result<Note, ConnectionErrorOrNonUtf8Path> {
        nvim_eval_and_cast!(title, nvim, r#"input("note name: ")"#, as_str, "vim function input( should always return a string");

        let now = chrono::Local::now();
        let note_id = now.format(&self.config.note_id_timestamp_format).to_string();

        let buf_path = {
            let mut p = self.config.home_path.clone();
            p.extend(&directories);
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
        buf.set_name(buf_path.to_str().ok_or_else(|| ConnectionErrorOrNonUtf8Path::NonUtf8Path)?).await?;
        buf.set_lines(0, 0, true, buf_contents).await?;
        buf.set_option("filetype", "wikipluginnote".into()).await?;

        if focus {
            nvim.set_current_buf(&buf).await?;
        }

        Ok(Note::new_physical(directories, note_id))
    }

    async fn open_index(&self, nvim: &mut Neovim<Compat<tokio::fs::File>>) -> Result<(), ConnectionErrorOrNonUtf8Path> {
        let index_path = self.config.home_path.join("index.md");
        let index_path: &str = index_path.to_str().ok_or(ConnectionErrorOrNonUtf8Path::NonUtf8Path)?;
        nvim.cmd(vec![("cmd".into(), "edit".into()), ("args".into(), vec![nvim_rs::Value::from(index_path)].into())], vec![]).await?;

        Ok(())
    }

    async fn new_note_and_insert_link(&self, nvim: &mut Neovim<Compat<tokio::fs::File>>) -> Result<(), InsertLinkError> {
        let new_note = self.new_note(nvim, Vec::new(), false).await?;
        self.insert_link_at_cursor(nvim, &new_note, None).await?;
        Ok(())
    }

    async fn insert_link_to_path_at_cursor_or_create(
        &self,
        nvim: &mut Neovim<Compat<tokio::fs::File>>,
        link_to: Option<String>,
        link_text: Option<String>,
    ) -> Result<(), InsertLinkError> {
        let n;
        let note = match link_to {
            Some(link_to_path) => {
                let path = Path::new(&link_to_path);
                n = Note::Physical(PhysicalNote::parse_from_filepath(&self.config, path)?);
                Some(&n)
            }
            None => None,
        };

        self.insert_link_at_cursor_or_create(nvim, note, link_text).await?;

        Ok(())
    }

    async fn insert_link_at_cursor_or_create(
        &self,
        nvim: &mut Neovim<Compat<tokio::fs::File>>,
        link_to: Option<&Note>,
        link_text: Option<String>,
    ) -> Result<(), InsertLinkError> {
        let note = match link_to {
            Some(link_to) => link_to,
            None => &self.new_note(nvim, Vec::new(), false).await?,
        };
        self.insert_link_at_cursor(nvim, note, link_text).await?;
        Ok(())
    }

    async fn insert_link_at_cursor(
        &self,
        nvim: &mut Neovim<Compat<tokio::fs::File>>,
        link_to: &Note,
        link_text: Option<String>,
    ) -> Result<(), InsertLinkError> {
        match link_to {
            Note::Physical(link_to) => {
                let link_text = match link_text {
                    Some(lt) => lt,
                    None => markdown::get_title(&markdown::parse_frontmatter(&markdown::parse_markdown(
                        &link_to.read_contents(&self.config, nvim).await?,
                    )?)?)
                    .unwrap_or_default(),
                };

                let current_note = Note::get_current_note(&self.config, nvim).await??;
                let link_path_text = links::format_link_path(&self.config, &current_note, &link_to.path(&self.config))?;
                nvim.put(vec![format!("[{link_text}]({link_path_text})")], "c", false, true).await?;

                Ok(())
            }
            Note::Scratch(_) => Err(InsertLinkError::CannotLinkToScratchNote)?,
        }
    }

    async fn open_tag_index(&self, nvim: &mut Neovim<Compat<tokio::fs::File>>) -> Result<(), TagIndexError> {
        let notes = self.list_all_physical_notes()?;
        let mut tag_table: BTreeMap<Tag, Vec<(&PhysicalNote, String, PathBuf)>> = BTreeMap::new(); // TODO: eventually this should become &(Note, String, PathBuf)
        let mut tag_list = BTreeSet::new();

        for note in &notes {
            let frontmatter = markdown::parse_frontmatter(&markdown::parse_markdown(&note.read_contents(&self.config, nvim).await?)?)?;
            let title = markdown::get_title(&frontmatter).unwrap_or_default();
            let tags = markdown::get_tags(&frontmatter).unwrap_or_default();
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
                    .set_lines(-2, -2, false, vec![format!("- [{}]({})", note_title, note_path.to_str().ok_or(TagIndexError::NonUtf8Path)?)])
                    .await?;
            }

            buffer.set_lines(-2, -2, false, vec!["".to_string()]).await?;
        }

        nvim.set_current_buf(&buffer).await?;

        Ok(())
    }

    async fn follow_link(&self, nvim: &mut Neovim<Compat<tokio::fs::File>>) -> Result<(), FollowLinkError> {
        let current_note = Note::get_current_note(&self.config, nvim).await??;
        let current_md = markdown::parse_markdown(&current_note.read_contents(&self.config, nvim).await?)?;

        nvim_eval_and_cast!(cursor_byte_index, nvim, r#"line2byte(line(".")) + col(".") - 1 - 1"#, as_u64, "byte index should be a number");
        let (_, link_path) = markdown::rec_find_preorder(&current_md, &mut |node| match node {
            ::markdown::mdast::Node::Link(::markdown::mdast::Link { children: _, position: Some(position), url, title: _ }) => {
                if markdown::point_in_position(position, cursor_byte_index.try_into().expect("byte index u64 does not fit into usize")) {
                    Some(url.to_string())
                } else {
                    None
                }
            }
            _ => None,
        })
        .ok_or(FollowLinkError::NotOnALink)?;

        let new_note_path = links::resolve_link_path(&self.config, &current_note, &link_path)?;

        nvim.cmd(
            vec![
                ("cmd".into(), "edit".into()),
                ("args".into(), vec![nvim_rs::Value::from(new_note_path.to_str().ok_or(FollowLinkError::NonUtf8Path)?)].into()),
            ],
            vec![],
        )
        .await?;

        Ok(())
    }

    async fn delete_note(&self, nvim: &mut Neovim<Compat<tokio::fs::File>>) -> Result<(), DeleteNoteError> {
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
            nvim.command(&format!(r#"echo "\n{} deleted""#, current_buf_path.to_str().ok_or(DeleteNoteError::NonUtf8CurrentBufferPath)?)).await?;
        } else {
            nvim.command(r#"echo "\nnot deleting""#).await?;
        }
        Ok(())
    }

    async fn regenerate_autogenerated_sections(&self, nvim: &mut Neovim<Compat<tokio::fs::File>>) -> Result<(), AutogenerateError> {
        let current_note = Note::get_current_note(&self.config, nvim).await??;
        let current_buf = nvim.get_current_buf().await?;

        let autogen_start_marker_regex =
            Regex::new(r#"\<wikiplugin_autogenerate\>\s*(\w+)(.*)"#).expect("autogenerate start marker regex should be valid");
        let autogen_end_marker_regex = Regex::new(r#"\<wikiplugin_autogenerate_end\>"#).expect("autogenerate end marker regex should be valid");

        let mut match_index = 0;
        loop {
            let buf_lines = current_buf.get_lines(0, -1, false).await?;

            let start_line = buf_lines
                .iter()
                .enumerate()
                .filter_map(|(line_number, line)| Some((line_number, autogen_start_marker_regex.captures(line)?)))
                .nth(match_index);
            let (start_line_nr, start_line_match) = match start_line {
                Some(start_line) => start_line,
                None => break,
            };

            let end_line = {
                let end_marker_line = buf_lines
                    .iter()
                    .enumerate()
                    .skip(start_line_nr + 1)
                    .find(|(_, line)| autogen_end_marker_regex.is_match(line))
                    .map(|(line_number, _)| line_number);
                let next_start_line = buf_lines
                    .iter()
                    .enumerate()
                    .skip(start_line_nr + 1)
                    .find(|(_, line)| autogen_start_marker_regex.is_match(line))
                    .map(|(line_number, _)| line_number);

                let insert_end_line = || async {
                    current_buf
                        .set_lines((start_line_nr + 1) as i64, (start_line_nr + 1) as i64, false, vec!["wikiplugin_autogenerate_end".to_string()])
                        .await?;
                    Ok::<_, AutogenerateError>(start_line_nr + 1)
                };

                match (end_marker_line, next_start_line) {
                    (None, _) => {
                        // if there is no end marker line, we insert an end marker line immediately after
                        insert_end_line().await?
                    }
                    (Some(end_marker_line), None) => {
                        // if there is an end marker line and no later start marker line, we replace until the end marker line
                        end_marker_line
                    }
                    (Some(end_marker_line), Some(next_start_line)) => {
                        // if there is both, it depends on which line comes first
                        if end_marker_line < next_start_line {
                            end_marker_line
                        } else {
                            // if the next start line comes first, then the end marker line actually applies to the next autogenerated section,
                            // so we have to insert an end marker line
                            insert_end_line().await?
                        }
                    }
                }
            };

            let autogenerate_command = start_line_match.get(1).expect("autogeneration regex match is missing command name (this should never happen because the regex always contains this capturing group)").as_str();
            let autogenerate_arguments = start_line_match
                .get(2)
                .expect("autogeneration start marker should have second capturing group (this should never happen because the regex always contains this capturing group)")
                .as_str()
                .split(";")
                .map(str::trim)
                .collect::<Vec<_>>();

            // TODO: full blown dsl with filters and pipes and things here?
            let replacement = match autogenerate_command {
                "index" => {
                    let directory: Vec<_> = autogenerate_arguments.first().copied().unwrap_or("").split("/").collect();
                    let sort_by = autogenerate_arguments.get(1).copied().unwrap_or("title");

                    let mut files = Vec::new();
                    for file in self.list_all_physical_notes()? {
                        if file.directories == directory {
                            let md = markdown::parse_markdown(&file.read_contents(&self.config, nvim).await?)?;
                            let frontmatter = markdown::parse_frontmatter(&md)?;
                            // TODO: having to do all of this is pretty messy but it is needed because the comparator cannot be async
                            let title = markdown::get_title(&frontmatter).ok();
                            let timestamp = markdown::get_timestamp(&frontmatter, &self.config)?;
                            files.push((file, title, timestamp))
                        }
                    }

                    let comparator = match sort_by {
                        "title" => |a: &(PhysicalNote, Option<String>, chrono::NaiveDateTime),
                                    b: &(PhysicalNote, Option<String>, chrono::NaiveDateTime)| {
                            if a.1.is_none() || b.1.is_none() {
                                a.0.id.cmp(&b.0.id)
                            } else {
                                a.1.cmp(&b.1)
                            }
                        },
                        "date" => |a: &(PhysicalNote, Option<String>, chrono::NaiveDateTime),
                                   b: &(PhysicalNote, Option<String>, chrono::NaiveDateTime)| { a.2.cmp(&b.2) },
                        "id" => |a: &(PhysicalNote, Option<String>, chrono::NaiveDateTime),
                                 b: &(PhysicalNote, Option<String>, chrono::NaiveDateTime)| { a.0.id.cmp(&b.0.id) },
                        _ => {
                            nvim.err_writeln(&format!("error: invalid comparison '{}'", sort_by)).await?;
                            |a: &(PhysicalNote, Option<String>, chrono::NaiveDateTime), b: &(PhysicalNote, Option<String>, chrono::NaiveDateTime)| {
                                a.0.id.cmp(&b.0.id)
                            }
                        }
                    };
                    files.sort_by(comparator);

                    let mut result = Vec::new();
                    for file in files {
                        let link_path = links::format_link_path(&self.config, &current_note, &file.0.path(&self.config))?;
                        result.push(format!("- [{}]({})", file.1.unwrap_or("".to_string()), link_path));
                    }

                    Some(result)
                }

                "backlinks" => {
                    // TODO: this is extremely slow
                    let current_note = Note::get_current_note(&self.config, nvim).await??;
                    let mut result = Vec::new();

                    for other_note in self.list_all_physical_notes()? {
                        if current_note.as_physical() == Some(&other_note) {
                            continue;
                        }

                        let other_note_contents = other_note.read_contents(&self.config, nvim).await?;
                        let other_note_markdown = markdown::parse_markdown(&other_note_contents)?;
                        let other_note_title = markdown::get_title(&markdown::parse_frontmatter(&other_note_markdown)?).unwrap_or_default();
                        let other_note_links = markdown::get_all_links(&other_note_markdown);

                        for link in other_note_links {
                            let link_to = links::resolve_link_path(&self.config, &Note::Physical(other_note.clone()), &link.url)?; // TODO: do not clone
                            if Some(&link_to) == current_note.path(&self.config).as_ref() {
                                result.push(format!(
                                    "- [{}]({})",
                                    other_note_title,
                                    links::format_link_path(&self.config, &current_note, &other_note.path(&self.config))?
                                ));
                                break;
                            }
                        }
                    }

                    Some(result)
                }

                "explore" => {
                    let root = Note::get_current_note(&self.config, nvim).await??;

                    let mut explored = BTreeSet::new();
                    let mut frontier = vec![root.clone()];
                    while let Some(current) = frontier.pop() {
                        let current_contents = current.read_contents(&self.config, nvim).await?;
                        let current_markdown = markdown::parse_markdown(&current_contents)?;
                        let current_links = markdown::get_all_links(&current_markdown);

                        for link in current_links {
                            let linked =
                                PhysicalNote::parse_from_filepath(&self.config, &links::resolve_link_path(&self.config, &current, &link.url)?)?;
                            let linked_as_note = Note::Physical(linked.clone()); // TODO: do not clone
                            if linked_as_note != root && !explored.contains(&linked) {
                                frontier.push(linked_as_note);
                                explored.insert(linked);
                            }
                        }
                    }

                    let mut result = Vec::new();

                    for note in explored {
                        let title = markdown::get_title(&markdown::parse_frontmatter(&markdown::parse_markdown(
                            &note.read_contents(&self.config, nvim).await?,
                        )?)?)
                        .unwrap_or_default();
                        result.push(format!("- [{}]({})", title, links::format_link_path(&self.config, &root, &note.path(&self.config))?));
                    }

                    Some(result)
                }

                _ => {
                    nvim.err_writeln(&format!("error: invalid autogenerate function '{}'", autogenerate_command)).await?;
                    None
                }
            };

            if let Some(replacement) = replacement {
                current_buf.set_lines((start_line_nr + 1) as i64, end_line as i64, false, replacement).await?;
            }

            match_index += 1;
        }

        Ok(())
    }

    fn list_all_physical_notes(&self) -> Result<Vec<PhysicalNote>, ListAllPhysicalNotesError> {
        glob::glob(&format!("{}/**/*.md", self.config.home_path.to_str().ok_or(ListAllPhysicalNotesError::NonUtf8HomePath)?))?
            .map(|path| {
                path.map_err(ListAllPhysicalNotesError::from)
                    .and_then(|path| PhysicalNote::parse_from_filepath(&self.config, &path).map_err(ListAllPhysicalNotesError::from))
            })
            .collect::<Result<Vec<_>, _>>()
    }
}
