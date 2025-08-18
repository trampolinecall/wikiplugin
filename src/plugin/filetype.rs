use std::{any::Any, collections::BTreeMap, fmt::Debug};

pub trait Filetype: Debug + Any {
    fn extension(&self) -> &'static str;
}

pub type SomeFiletype = &'static dyn Filetype;

pub struct FiletypeRegistry {
    registered: Vec<SomeFiletype>,
    extension_mapping: BTreeMap<&'static str, SomeFiletype>,
}

impl FiletypeRegistry {
    pub fn new() -> Self {
        Self { registered: Vec::new(), extension_mapping: BTreeMap::new() }
    }

    pub fn register(&mut self, ft: SomeFiletype) {
        self.registered.push(ft);
        self.extension_mapping.insert(ft.extension(), ft);
    }

    pub fn look_up_extension(&self, ext: &str) -> Option<SomeFiletype> {
        self.extension_mapping.get(ext).copied()
    }
}
