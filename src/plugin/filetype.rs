use std::{collections::BTreeMap, fmt::Debug};

#[derive(Debug)]
pub struct Filetype {
    pub extension: String,
}

#[derive(Debug, Clone, Copy)]
pub struct SomeFiletype<'fr>(&'fr Filetype);
impl<'fr> PartialEq for SomeFiletype<'fr> {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.0, other.0)
    }
}
impl<'fr> Eq for SomeFiletype<'fr> {}
impl<'fr> PartialOrd for SomeFiletype<'fr> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl<'fr> Ord for SomeFiletype<'fr> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.0 as *const Filetype).addr().cmp(&(other.0 as *const Filetype).addr())
    }
}

pub struct FiletypeRegistry {
    registered: BTreeMap<String, Filetype>,
}

impl FiletypeRegistry {
    pub fn new() -> Self {
        Self { registered: BTreeMap::new() }
    }

    pub fn register(&mut self, ft: Filetype) {
        self.registered.insert(ft.extension.clone(), ft);
    }

    pub fn look_up_extension(&self, ext: &str) -> Option<SomeFiletype> {
        self.registered.get(ext).map(SomeFiletype)
    }
}
