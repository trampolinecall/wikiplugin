use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use nvim_oxi::{api, Dictionary};

use crate::plugin::note::{Note, PhysicalNote, Tag};

mod links;
mod markdown;
pub mod note;

#[derive(Debug)]
pub struct ConfigDictMissingKey(&'static str);
impl std::error::Error for ConfigDictMissingKey {}
impl std::fmt::Display for ConfigDictMissingKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "config dict is missing key {}", self.0)
    }
}
#[derive(Debug)]
pub struct HomePathNotAbsolute;
impl std::error::Error for HomePathNotAbsolute {}
impl std::fmt::Display for HomePathNotAbsolute {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "home path should be absolute")
    }
}
error_union! {
    pub enum ConfigParseError {
        ConversionError(nvim_oxi::conversion::Error),
        ConfigDictMissingKey(ConfigDictMissingKey),
        HomePathNotAbsolute(HomePathNotAbsolute),
    }
}

#[derive(Clone)]
pub struct Config {
    home_path: PathBuf,
    note_id_timestamp_format: String,
    date_format: String,
    time_format: String,
}
impl Config {
    pub fn parse_from_dict(dict: Dictionary) -> Result<Config, ConfigParseError> {
        fn get_from_dict<T: nvim_oxi::conversion::FromObject>(dict: &Dictionary, key: &'static str) -> Result<T, ConfigParseError> {
            Ok(T::from_object(dict.get(key).ok_or(ConfigDictMissingKey(key))?.clone())?)
        }
        let home_path: PathBuf = get_from_dict::<String>(&dict, "home_path")?.into();
        if !home_path.is_absolute() {
            Err(HomePathNotAbsolute)?;
        }
        let c = Config {
            home_path,
            note_id_timestamp_format: get_from_dict(&dict, "note_id_timestamp_format")?,
            date_format: get_from_dict(&dict, "date_format")?,
            time_format: get_from_dict(&dict, "time_format")?,
        };
        Ok(c)
    }
}

#[derive(Debug)]
pub struct NonUtf8Path;
impl std::error::Error for NonUtf8Path {}
impl std::fmt::Display for NonUtf8Path {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "path is not utf8")
    }
}
error_union! {
    pub enum ApiErrorOrNonUtf8Path {
        ApiError(api::Error),
        NonUtf8Path(NonUtf8Path),
    }
}

#[derive(Debug)]
pub struct CannotLinkToScratchNote;
impl std::error::Error for CannotLinkToScratchNote {}
impl std::fmt::Display for CannotLinkToScratchNote {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "cannot link to scratch note")
    }
}
error_union! {
    pub enum InsertLinkError {
        ParseFromFilepathError(note::ParseFromFilepathError),
        GetCurrentNoteError(note::GetCurrentNoteError),
        FormatLinkPathError(links::FormatLinkPathError),
        ApiError(api::Error),
        NonUtf8Path(NonUtf8Path),
        CannotLinkToScratchNote(CannotLinkToScratchNote),
    }
}
convert_error_union! {
    ApiErrorOrNonUtf8Path => InsertLinkError {
        ApiError => ApiError,
        NonUtf8Path => NonUtf8Path
    }
}

error_union! {
    pub enum TagIndexError {
        ListAllPhysicalNotesError(ListAllPhysicalNotesError),
        ReadContentsError(note::ReadContentsError),
        GetCurrentNoteError(note::GetCurrentNoteError),
        ApiError(api::Error),
        NonUtf8Path(NonUtf8Path),
        ParseMarkdownError(markdown::MdParseError), // TODO: remove these? if the frontmatter or title is incorrect just put nothing
        InvalidFrontmatter(markdown::InvalidFrontmatter),
    }
}

#[derive(Debug)]
pub struct NotOnALink;
impl std::error::Error for NotOnALink {}
impl std::fmt::Display for NotOnALink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "not on a link")
    }
}
error_union! {
    pub enum FollowLinkError {
        ApiError(api::Error),
        GetCurrentNoteError(note::GetCurrentNoteError),
        ReadContentsError(note::ReadContentsError),
        ParseFromFilepathError(note::ParseFromFilepathError),
        ParseMarkdownError(markdown::MdParseError),
        NotOnALink(NotOnALink),
        ResolveLinkPathError(links::ResolveLinkPathError),
        NonUtf8Path(NonUtf8Path),
    }
}

error_union! {
    pub enum DeleteNoteError {
        ApiError(api::Error),
        NonUtf8Path(NonUtf8Path),
        IoError(std::io::Error),
    }
}

error_union! {
    pub enum AutogenerateError {
        ApiError(api::Error),
        ListAllPhysicalNotesError(ListAllPhysicalNotesError),
        MdParseError(markdown::MdParseError), // TODO: remove most of these errors and just dont list files that trigger them?
        ReadContentsError(note::ReadContentsError),
        InvalidFrontmatter(markdown::InvalidFrontmatter),
        GetFrontmatterFieldError(markdown::GetFrontmatterFieldError),
        GetTimestampError(markdown::GetTimestampError),
        FormatLinkPathError(links::FormatLinkPathError),
        ResolveLinkPathError(links::ResolveLinkPathError),
        ParseFromFilepathError(note::ParseFromFilepathError),
        GetCurrentNoteError(note::GetCurrentNoteError),
    }
}

error_union! {
    pub enum ListAllPhysicalNotesError {
        NonUtf8Path(NonUtf8Path),
        GlobPatternError(glob::PatternError),
        GlobError(glob::GlobError),
        ParseFromFilepathError(note::ParseFromFilepathError),
    }
}

pub fn new_note(config: &Config, directories: Vec<String>, focus: bool) -> Result<Note, ApiErrorOrNonUtf8Path> {
    let title: String = nvim_oxi::api::eval(r#"input("note name: ")"#)?;

    let now = chrono::Local::now();
    let note_id = now.format(&config.note_id_timestamp_format).to_string();

    let buf_path = {
        let mut p = config.home_path.clone();
        p.extend(&directories);
        p.push(&note_id);
        p.set_extension("md");
        p
    };

    // TODO: customizable templates?
    let buf_contents = [
        "---".to_string(),
        format!("title: {title}"),
        format!("date: {}", now.format(&config.date_format)),
        format!("time: {}", now.format(&config.time_format)),
        "tags:".to_string(),
        "---".to_string(),
    ]
    .to_vec();

    let mut buf = api::create_buf(true, false)?;
    buf.set_name(buf_path.to_str().ok_or(NonUtf8Path)?)?;
    buf.set_lines(0..0, true, buf_contents)?;
    buf.set_option("filetype", "wikipluginnote")?;

    if focus {
        api::set_current_buf(&buf)?;
    }

    Ok(Note::new_physical(directories, note_id))
}

pub fn open_index(config: &Config) -> Result<(), ApiErrorOrNonUtf8Path> {
    let index_path = config.home_path.join("index.md");
    let index_path: &str = index_path.to_str().ok_or(NonUtf8Path)?;
    api::cmd(&api::types::CmdInfos::builder().cmd("edit").args([index_path]).build(), &api::opts::CmdOpts::default())?;

    Ok(())
}

pub fn new_note_and_insert_link(config: &Config) -> Result<(), InsertLinkError> {
    let new_note = new_note(config, Vec::new(), false)?;
    insert_link_at_cursor(config, &new_note, None)?;
    Ok(())
}

pub fn insert_link_to_path_at_cursor_or_create(config: &Config, link_to: Option<String>, link_text: Option<String>) -> Result<(), InsertLinkError> {
    let n;
    let note = match link_to {
        Some(link_to_path) => {
            let path = Path::new(&link_to_path);
            n = Note::Physical(PhysicalNote::parse_from_filepath(config, path)?);
            Some(&n)
        }
        None => None,
    };

    insert_link_at_cursor_or_create(config, note, link_text)?;

    Ok(())
}

pub fn insert_link_at_cursor_or_create(config: &Config, link_to: Option<&Note>, link_text: Option<String>) -> Result<(), InsertLinkError> {
    let note = match link_to {
        Some(link_to) => link_to,
        None => &new_note(config, Vec::new(), false)?,
    };
    insert_link_at_cursor(config, note, link_text)?;
    Ok(())
}

pub fn insert_link_at_cursor(config: &Config, link_to: &Note, link_text: Option<String>) -> Result<(), InsertLinkError> {
    match link_to {
        Note::Physical(link_to) => {
            let link_text = match link_text {
                Some(lt) => lt,
                None => link_to
                    .read_contents(config)
                    .ok()
                    .and_then(|contents| markdown::parse_markdown(&contents).ok())
                    .and_then(|markdown| markdown::parse_frontmatter(&markdown).ok())
                    .and_then(|frontmatter| markdown::get_title(&frontmatter).ok())
                    .unwrap_or_default(),
            };

            let current_note = Note::get_current_note(config)?;
            let link_path_text = links::format_link_path(config, &current_note, &link_to.path(config))?;
            api::put([format!("[{link_text}]({link_path_text})")].into_iter(), api::types::RegisterType::Charwise, false, true)?;

            Ok(())
        }
        Note::Scratch(_) => Err(CannotLinkToScratchNote)?,
    }
}

pub fn open_tag_index(config: &Config) -> Result<(), TagIndexError> {
    let notes = list_all_physical_notes(config)?;
    let mut tag_table: BTreeMap<Tag, Vec<(&PhysicalNote, String, PathBuf)>> = BTreeMap::new(); // TODO: eventually this should become &(Note, String, PathBuf)
    let mut tag_list = BTreeSet::new();

    for note in &notes {
        let frontmatter = markdown::parse_frontmatter(&markdown::parse_markdown(&note.read_contents(config)?)?)?; // TODO: do not error out on these and just don't list these files?
        let title = markdown::get_title(&frontmatter).unwrap_or_default();
        let tags = markdown::get_tags(&frontmatter).unwrap_or_default();
        let path = note.path(config);

        for tag in tags {
            tag_table.entry(tag.clone()).or_default().push((note, title.clone(), path.clone()));
            tag_list.insert(tag);
        }
    }

    let mut buffer = api::create_buf(true, true)?;
    buffer.set_option("filetype", "wikipluginnote")?;

    let mut lines = Vec::new();
    for tag in tag_list {
        lines.extend([format!("# {}", tag), "".to_string()]);
        for (_, note_title, note_path) in &tag_table[&tag] {
            lines.extend([format!("- [{}]({})", note_title, note_path.to_str().ok_or(NonUtf8Path)?)]);
        }
        lines.extend(["".to_string()]);
    }

    buffer.set_lines(0..0, false, lines)?;
    api::set_current_buf(&buffer)?;

    Ok(())
}

pub fn follow_link(config: &Config) -> Result<(), FollowLinkError> {
    let current_note = Note::get_current_note(config)?;
    let current_md = markdown::parse_markdown(&current_note.read_contents(config)?)?;

    let cursor_byte_index: usize = nvim_oxi::api::eval(r#"line2byte(line(".")) + col(".") - 1 - 1"#)?;
    let (_, link_path) = markdown::rec_find_preorder(&current_md, &mut |node| match node {
        ::markdown::mdast::Node::Link(::markdown::mdast::Link { children: _, position: Some(position), url, title: _ }) => {
            if markdown::point_in_position(position, cursor_byte_index) {
                Some(url.to_string())
            } else {
                None
            }
        }
        _ => None,
    })
    .ok_or(NotOnALink)?;

    let new_note_path = links::resolve_link_path(config, &current_note, &link_path)?;

    api::cmd(
        &api::types::CmdInfos::builder().cmd("edit").args([new_note_path.to_str().ok_or(NonUtf8Path)?]).build(),
        &api::opts::CmdOpts::default(),
    )?;

    Ok(())
}

pub fn delete_note() -> Result<(), DeleteNoteError> {
    let current_buf_path_str: String = nvim_oxi::api::eval(r#"expand("%:p")"#)?;
    let current_buf_path = Path::new(&current_buf_path_str);

    let choice: String =
        nvim_oxi::api::eval(r#"input("are you sure you want to delete this note?\noptions: 'yes' for yes, anything else for no\ninput: ")"#)?;
    if choice == "yes" {
        std::fs::remove_file(current_buf_path)?;
        api::command(&format!(r#"echo "\n{} deleted""#, current_buf_path.to_str().ok_or(NonUtf8Path)?))?;
    } else {
        api::command(r#"echo "\nnot deleting""#)?;
    }
    Ok(())
}

pub fn regenerate_autogenerated_sections(config: &Config) -> Result<(), AutogenerateError> {
    let current_note = Note::get_current_note(config)?;
    let mut current_buf = api::get_current_buf();

    let autogen_start_marker_regex = r#"\<wikiplugin_autogenerate\>\s*\(\w\+\)\(.*\)"#;
    let autogen_end_marker_regex = r#"\<wikiplugin_autogenerate_end\>"#;

    let mut match_index = 1;

    let negative_one_to_option = |x: isize| -> Option<usize> {
        if x == -1 {
            None
        } else {
            Some(x as usize)
        }
    };

    while let Some(start_line_index) =
        negative_one_to_option(api::eval(&format!("match(getline(0, '$'), '{}', 0, {})", autogen_start_marker_regex, match_index))?)
    {
        let start_matches: Vec<String> = api::eval(&format!("matchlist(getline(0, '$'), '{}', 0, {})", autogen_start_marker_regex, match_index))?;

        let end_line_index = {
            let end_marker_line_index =
                negative_one_to_option(api::eval(&format!("match(getline(0, '$'), '{}', {})", autogen_end_marker_regex, start_line_index + 1))?);

            let next_start_line_index =
                negative_one_to_option(api::eval(&format!("match(getline(0, '$'), '{}', {})", autogen_start_marker_regex, start_line_index + 1))?);

            let mut insert_end_line = || {
                current_buf.set_lines(start_line_index + 1..start_line_index + 1, false, vec!["wikiplugin_autogenerate_end".to_string()])?;
                Ok::<_, AutogenerateError>(start_line_index + 1)
            };

            match (end_marker_line_index, next_start_line_index) {
                (None, _) => {
                    // if there is no end marker line, we insert an end marker line immediately after
                    insert_end_line()?
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
                        // if the next start line comes first, then the end marker line actually applies to that next autogenerated section,
                        // so we have to insert an end marker line
                        insert_end_line()?
                    }
                }
            }
        };

        let autogenerate_command = start_matches
            .get(1)
            .expect("autogeneration is missing command name (this should never happen because the regex always contains this capturing group)")
            .as_str();
        let autogenerate_arguments = start_matches
            .get(2)
            .expect("autogeneration start marker should have second capturing group")
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
                for file in list_all_physical_notes(config)? {
                    if file.directories == directory {
                        let md = markdown::parse_markdown(&file.read_contents(config)?)?; // TODO: don't error on this?
                        let frontmatter = markdown::parse_frontmatter(&md).ok();
                        let title = frontmatter.as_ref().and_then(|f| markdown::get_title(f).ok());
                        files.push((file, md, frontmatter, title))
                    }
                }

                type ComparatorTuple = (PhysicalNote, ::markdown::mdast::Node, Option<yaml_rust::Yaml>, Option<String>);
                let comparator = match sort_by {
                    "title" => {
                        (&|(a, _, _, a_title): &ComparatorTuple, (b, _, _, b_title): &ComparatorTuple| {
                            if a_title.is_none() || b_title.is_none() {
                                a.id.cmp(&b.id)
                            } else {
                                a_title.cmp(b_title)
                            }
                        }) as &dyn Fn(&ComparatorTuple, &ComparatorTuple) -> _
                    }
                    "date" => &|(_, _, a_frontmatter, _): &ComparatorTuple, (_, _, b_frontmatter, _): &ComparatorTuple| {
                        let a_timestamp = a_frontmatter.as_ref().and_then(|f| markdown::get_timestamp(f, config).ok());
                        let b_timestamp = b_frontmatter.as_ref().and_then(|f| markdown::get_timestamp(f, config).ok());
                        a_timestamp.cmp(&b_timestamp)
                    },
                    "id" => &|(a, _, _, _): &ComparatorTuple, (b, _, _, _): &ComparatorTuple| a.id.cmp(&b.id),
                    _ => {
                        api::err_writeln(&format!("error: invalid comparison '{}'", sort_by));
                        &|(a, _, _, _): &ComparatorTuple, (b, _, _, _): &ComparatorTuple| a.id.cmp(&b.id)
                    }
                };
                files.sort_by(comparator);

                let mut result = Vec::new();
                for (file, _, _, title) in files {
                    let link_path = links::format_link_path(config, &current_note, &file.path(config))?;
                    result.push(format!("- [{}]({})", title.unwrap_or("".to_string()), link_path));
                }

                Some(result)
            }

            "backlinks" => {
                // TODO: this is extremely slow
                let current_note = Note::get_current_note(config)?;
                let mut result = Vec::new();

                for other_note in list_all_physical_notes(config)? {
                    if current_note.as_physical() == Some(&other_note) {
                        continue;
                    }

                    let other_note_contents = other_note.read_contents(config)?; // TODO: don't error out on this?
                    let other_note_markdown = markdown::parse_markdown(&other_note_contents)?; // TODO: don't error out on this?
                    let other_note_title = markdown::get_title(&markdown::parse_frontmatter(&other_note_markdown)?).unwrap_or_default(); // TODO: don't error out on this?
                    let other_note_links = markdown::get_all_links(&other_note_markdown);

                    for link in other_note_links {
                        let link_to = links::resolve_link_path(config, &Note::Physical(other_note.clone()), &link.url)?; // TODO: do not clone
                        if Some(&link_to) == current_note.path(config).as_ref() {
                            result.push(format!(
                                "- [{}]({})",
                                other_note_title,
                                links::format_link_path(config, &current_note, &other_note.path(config))?
                            ));
                            break;
                        }
                    }
                }

                Some(result)
            }

            "explore" => {
                let root = Note::get_current_note(config)?;

                let mut explored = BTreeSet::new();
                let mut frontier = vec![root.clone()];
                while let Some(current) = frontier.pop() {
                    let current_contents = current.read_contents(config)?; // TODO: don't error out on this?
                    let current_markdown = markdown::parse_markdown(&current_contents)?; // TODO: don't error out on this?
                    let current_links = markdown::get_all_links(&current_markdown);

                    for link in current_links {
                        let linked = PhysicalNote::parse_from_filepath(config, &links::resolve_link_path(config, &current, &link.url)?)?; // TODO: don't error out on this
                        let linked_as_note = Note::Physical(linked.clone()); // TODO: do not clone
                        if linked_as_note != root && !explored.contains(&linked) {
                            frontier.push(linked_as_note);
                            explored.insert(linked);
                        }
                    }
                }

                let mut result = Vec::new();

                for note in explored {
                    let title = note
                        .read_contents(config)
                        .ok()
                        .and_then(|contents| markdown::parse_markdown(&contents).ok())
                        .and_then(|markdown| markdown::parse_frontmatter(&markdown).ok())
                        .and_then(|frontmatter| markdown::get_title(&frontmatter).ok())
                        .unwrap_or_default();

                    result.push(format!("- [{}]({})", title, links::format_link_path(config, &root, &note.path(config))?));
                }

                Some(result)
            }

            _ => {
                api::err_writeln(&format!("error: invalid autogenerate function '{}'", autogenerate_command));
                None
            }
        };

        if let Some(replacement) = replacement {
            current_buf.set_lines((start_line_index + 1)..end_line_index, false, replacement)?;
        }

        match_index += 1;
    }

    Ok(())
}

fn list_all_physical_notes(config: &Config) -> Result<Vec<PhysicalNote>, ListAllPhysicalNotesError> {
    glob::glob(&format!("{}/**/*.md", config.home_path.to_str().ok_or(NonUtf8Path)?))?
        .map(|path| {
            path.map_err(ListAllPhysicalNotesError::from)
                .and_then(|path| PhysicalNote::parse_from_filepath(config, &path).map_err(ListAllPhysicalNotesError::from))
        })
        .collect::<Result<Vec<_>, _>>()
}
