use nvim_oxi::{Dictionary, Function, Object};

use crate::plugin::{note::Note, Config};

mod connection;
mod plugin;

#[nvim_oxi::plugin]
fn wikiplugin_internal() -> Dictionary {
    flexi_logger::Logger::try_with_env()
        .expect("could not initialize logger")
        .log_to_file(flexi_logger::FileSpec::default())
        .start()
        .expect("could not initialize logger");

    Dictionary::from_iter([
        (
            "new_note",
            Object::from(Function::from_fn(|(config, directories, focus): (Dictionary, Vec<String>, bool)| {
                do_function(config, move |config| plugin::new_note(&config, directories, focus).map(|_| ()))
            })),
        ),
        ("open_index", Object::from(Function::from_fn(|config: Dictionary| do_function(config, |config| plugin::open_index(&config))))),
        ("delete_note", Object::from(Function::from_fn(|config: Dictionary| do_function(config, |config| plugin::delete_note())))),
        (
            "new_note_and_insert_link",
            Object::from(Function::from_fn(|config: Dictionary| do_function(config, |config| plugin::new_note_and_insert_link(&config)))),
        ),
        ("open_tag_index", Object::from(Function::from_fn(|config: Dictionary| do_function(config, |config| plugin::open_tag_index(&config))))),
        ("follow_link", Object::from(Function::from_fn(|config: Dictionary| do_function(config, |config| plugin::follow_link(&config))))),
        (
            "insert_link_at_cursor",
            Object::from(Function::from_fn(
                |(config, link_to_directories, link_to_id, link_text): (Dictionary, Vec<String>, String, Option<String>)| {
                    do_function(config, |config| {
                        // TODO: move this logic somewhere else
                        plugin::insert_link_at_cursor(&config, &Note::new_physical(link_to_directories, link_to_id), link_text)
                    })
                },
            )),
        ),
        (
            "insert_link_at_cursor_or_create",
            Object::from(Function::from_fn(
                |(config, link_to_directories, link_to_id, link_text): (Dictionary, Vec<String>, Option<String>, Option<String>)| {
                    let n;
                    let note = match link_to_id {
                        Some(link_to_id) => {
                            n = Note::new_physical(link_to_directories, link_to_id); // TODO: move this logic somewhere else
                            Some(&n)
                        }
                        None => None,
                    };

                    do_function(config, |config| plugin::insert_link_at_cursor_or_create(&config, note, link_text))
                },
            )),
        ),
        (
            "insert_link_to_path_at_cursor_or_create",
            Object::from(Function::from_fn(|(config, link_to_path, link_text): (Dictionary, Option<String>, Option<String>)| {
                do_function(config, |config| plugin::insert_link_to_path_at_cursor_or_create(&config, link_to_path, link_text))
            })),
        ),
        (
            "regenerate_autogenerated_sections",
            Object::from(Function::from_fn(|config: Dictionary| do_function(config, |config| plugin::regenerate_autogenerated_sections(&config)))),
        ),
    ])
}

// TODO: move this somewhere else
fn do_function(config: Dictionary, r: impl FnOnce(Config) -> Result<(), anyhow::Error>) {
    match Config::parse_from_dict(config).and_then(r) {
        Ok(()) => (),
        Err(e) => connection::print_error(e),
    }
}
