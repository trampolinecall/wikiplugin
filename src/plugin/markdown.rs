use markdown::mdast::Node;
use yaml_rust::Yaml;

use crate::{
    error::Error,
    plugin::{note::Tag, Config},
};

pub fn find_frontmatter(md: &Node) -> Result<String, Error> {
    Ok(rec_find_preorder(md, &mut |node| match node {
        Node::Yaml(yaml) => Some(yaml.value.clone()),
        _ => None,
    })
    .ok_or("could not find frontmatter in file")?
    .1)
}

pub fn parse_frontmatter(md: &Node) -> Result<Yaml, Error> {
    // TODO: swap_remove will panic if the yaml parser does not output any documents (i am not sure how that will happen though)
    Ok(yaml_rust::YamlLoader::load_from_str(&find_frontmatter(md)?)?.swap_remove(0))
}

pub fn get_title(frontmatter: &Yaml) -> Result<String, Error> {
    Ok(frontmatter
        .as_hash()
        .ok_or("frontmatter is not hash table at the top level")?
        .get(&Yaml::String("title".to_string()))
        .ok_or("frontmatter has no title field")?
        .as_str()
        .ok_or("title is not string")?
        .to_string())
}

pub fn get_timestamp(frontmatter: &Yaml, config: &Config) -> Result<chrono::NaiveDateTime, Error> {
    let frontmatter = frontmatter.as_hash().ok_or("frontmatter is not hash table at the top level")?;
    let date = frontmatter
        .get(&Yaml::String("date".to_string()))
        .ok_or("frontmatter has no date field")?
        .as_str()
        .ok_or("date field is not string")?
        .to_string();
    let time = frontmatter.get(&Yaml::String("time".to_string()));

    let date = chrono::NaiveDate::parse_from_str(&date, &config.date_format)?;
    let time = match time {
        Some(time) => chrono::NaiveTime::parse_from_str(time.as_str().ok_or("time field is not string")?, &config.time_format)?,
        None => chrono::NaiveTime::MIN,
    };

    Ok(chrono::NaiveDateTime::new(date, time))
}

pub fn get_tags(frontmatter: &Yaml) -> Result<Vec<Tag>, Error> {
    let s = frontmatter
        .as_hash()
        .ok_or("frontmatter is not hash table at the top level")?
        .get(&Yaml::String("tags".to_string()))
        .ok_or("frontmatter has no tags field")?;
    match s {
        Yaml::String(s) => Ok(s.split(" ").map(Tag::parse_from_str).collect()),
        Yaml::Array(vec) => Ok(vec
            .iter()
            .map(|tag| Some(Tag::parse_from_str(tag.as_str()?)))
            .collect::<Option<Vec<_>>>()
            .ok_or("tags field is not array of strings")?),
        _ => Err("tags field is not string or array".to_string().into()),
    }
}

pub fn rec_find_preorder<'md, R>(node: &'md Node, pred: &mut impl FnMut(&Node) -> Option<R>) -> Option<(&'md Node, R)> {
    pred(node).map(|r| (node, r)).or_else(|| node.children().into_iter().flatten().find_map(|sn| rec_find_preorder(sn, pred)))
}
pub fn rec_find_postorder<'md, R>(node: &'md Node, pred: &mut impl FnMut(&Node) -> Option<R>) -> Option<(&'md Node, R)> {
    node.children().into_iter().flatten().find_map(|sn| rec_find_postorder(sn, pred)).or_else(|| pred(node).map(|r| (node, r)))
}

pub fn point_in_position(position: &markdown::unist::Position, byte_index: usize) -> bool {
    byte_index >= position.start.offset && byte_index < position.end.offset
}
