use markdown::{mdast, to_mdast};
use yaml_rust::Yaml;

use crate::plugin::{note::Tag, Config};

#[derive(Debug)]
pub struct MdParseError(markdown::message::Message);
impl std::fmt::Display for MdParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for MdParseError {}

#[derive(Debug)]
pub struct NoFrontmatter;
impl std::fmt::Display for NoFrontmatter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "no frontmatter in file")
    }
}

error_union! {
    pub enum InvalidFrontmatter {
        NoFrontmatter(NoFrontmatter),
        YamlScanError(yaml_rust::ScanError),
    }
}

pub fn parse_markdown(contents: &str) -> Result<mdast::Node, MdParseError> {
    to_mdast(
        contents,
        &markdown::ParseOptions {
            constructs: markdown::Constructs { frontmatter: true, ..markdown::Constructs::gfm() },
            ..markdown::ParseOptions::gfm()
        },
    )
    .map_err(MdParseError)
}
pub fn find_frontmatter(md: &mdast::Node) -> Result<String, NoFrontmatter> {
    Ok(rec_find_preorder(md, &mut |node| match node {
        mdast::Node::Yaml(yaml) => Some(yaml.value.clone()),
        _ => None,
    })
    .ok_or(NoFrontmatter)?
    .1)
}

pub fn parse_frontmatter(md: &mdast::Node) -> Result<Yaml, InvalidFrontmatter> {
    // TODO: swap_remove will panic if the yaml parser does not output any documents (i am not sure how that will happen though)
    Ok(yaml_rust::YamlLoader::load_from_str(&find_frontmatter(md)?)?.swap_remove(0))
}

#[derive(Debug)]
pub enum GetFrontmatterFieldError {
    NotHashTable,
    NoField(&'static str),
    FieldWrongType { expected_type: &'static str },
}
impl std::fmt::Display for GetFrontmatterFieldError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GetFrontmatterFieldError::NotHashTable => write!(f, "frontmatter is not hash table"),
            GetFrontmatterFieldError::NoField(field) => write!(f, "no field called '{field}'"),
            GetFrontmatterFieldError::FieldWrongType { expected_type } => write!(f, "frontmatter field type is not {expected_type}"),
        }
    }
}
pub fn get_title(frontmatter: &Yaml) -> Result<String, GetFrontmatterFieldError> {
    Ok(frontmatter
        .as_hash()
        .ok_or(GetFrontmatterFieldError::NotHashTable)?
        .get(&Yaml::String("title".to_string()))
        .ok_or(GetFrontmatterFieldError::NoField("title"))?
        .as_str()
        .ok_or(GetFrontmatterFieldError::FieldWrongType { expected_type: "string" })?
        .to_string())
}

#[derive(Debug)]
pub enum GetTimestampError {
    NotHashTable,
    NoDateField,
    TimestampFieldsNotString,
    TimestampParseError(chrono::ParseError),
}
impl std::fmt::Display for GetTimestampError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GetTimestampError::NotHashTable => write!(f, "frontmatter is not hash table"),
            GetTimestampError::NoDateField => write!(f, "no date field"),
            GetTimestampError::TimestampFieldsNotString => write!(f, "timestamp fields are not a string"),
            GetTimestampError::TimestampParseError(e) => e.fmt(f),
        }
    }
}
pub fn get_timestamp(frontmatter: &Yaml, config: &Config) -> Result<chrono::NaiveDateTime, GetTimestampError> {
    let frontmatter = frontmatter.as_hash().ok_or(GetTimestampError::NotHashTable)?;
    let date = frontmatter
        .get(&Yaml::String("date".to_string()))
        .ok_or(GetTimestampError::NoDateField)?
        .as_str()
        .ok_or(GetTimestampError::TimestampFieldsNotString)?
        .to_string();
    let time = frontmatter.get(&Yaml::String("time".to_string()));

    let date = chrono::NaiveDate::parse_from_str(&date, &config.date_format).map_err(GetTimestampError::TimestampParseError)?;
    let time = match time {
        Some(time) => chrono::NaiveTime::parse_from_str(time.as_str().ok_or(GetTimestampError::TimestampFieldsNotString)?, &config.time_format)
            .map_err(GetTimestampError::TimestampParseError)?,
        None => chrono::NaiveTime::MIN,
    };

    Ok(chrono::NaiveDateTime::new(date, time))
}

pub fn get_tags(frontmatter: &Yaml) -> Result<Vec<Tag>, GetFrontmatterFieldError> {
    let s = frontmatter
        .as_hash()
        .ok_or(GetFrontmatterFieldError::NotHashTable)?
        .get(&Yaml::String("tags".to_string()))
        .ok_or(GetFrontmatterFieldError::NoField("tags"))?;
    match s {
        Yaml::String(s) => Ok(s.split(" ").map(Tag::parse_from_str).collect()),
        Yaml::Array(vec) => Ok(vec
            .iter()
            .map(|tag| Some(Tag::parse_from_str(tag.as_str()?)))
            .collect::<Option<Vec<_>>>()
            .ok_or(GetFrontmatterFieldError::FieldWrongType { expected_type: "array of strings (or string)" })?),
        _ => Err(GetFrontmatterFieldError::FieldWrongType { expected_type: "array of strings or string" }),
    }
}

pub fn get_all_links(md: &mdast::Node) -> Vec<&mdast::Link> {
    /* TODO: these lifetimes do not work out
    fn is_link(node: &mdast::Node) -> Option<&mdast::Link> {
        match node {
            mdast::Node::Link(link) => Some(link),
            _ => None,
        }
    }
    rec_filter_preorder(md, is_link)
    */

    fn is_link(node: &mdast::Node) -> Option<&mdast::Link> {
        match node {
            mdast::Node::Link(link) => Some(link),
            _ => None,
        }
    }
    fn helper<'md>(acc: &mut Vec<&'md mdast::Link>, node: &'md mdast::Node) {
        if let Some(res) = is_link(node) {
            acc.push(res)
        }

        for child in node.children().into_iter().flatten() {
            helper(acc, child);
        }
    }
    let mut result = Vec::new();
    helper(&mut result, md);
    result
}

pub fn rec_filter_preorder<R>(node: &mdast::Node, mut pred: impl for<'a> FnMut(&'a mdast::Node) -> Option<R>) -> Vec<R> {
    fn helper<R>(acc: &mut Vec<R>, pred: &mut impl FnMut(&mdast::Node) -> Option<R>, node: &mdast::Node) {
        if let Some(res) = pred(node) {
            acc.push(res)
        }

        for child in node.children().into_iter().flatten() {
            helper(acc, pred, child);
        }
    }
    let mut result = Vec::new();
    helper(&mut result, &mut pred, node);
    result
}
pub fn rec_find_preorder<'md, R>(node: &'md mdast::Node, pred: &mut impl FnMut(&mdast::Node) -> Option<R>) -> Option<(&'md mdast::Node, R)> {
    pred(node).map(|r| (node, r)).or_else(|| node.children().into_iter().flatten().find_map(|sn| rec_find_preorder(sn, pred)))
}
pub fn rec_find_postorder<'md, R>(node: &'md mdast::Node, pred: &mut impl FnMut(&mdast::Node) -> Option<R>) -> Option<(&'md mdast::Node, R)> {
    node.children().into_iter().flatten().find_map(|sn| rec_find_postorder(sn, pred)).or_else(|| pred(node).map(|r| (node, r)))
}

pub fn point_in_position(position: &markdown::unist::Position, byte_index: usize) -> bool {
    byte_index >= position.start.offset && byte_index < position.end.offset
}
