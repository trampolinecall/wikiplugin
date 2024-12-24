trait FromNvimValue: Sized {
    fn from(message_name: &str, argument_name: &str, v: &nvim_rs::Value) -> Result<Self, String> {
        let converted = Self::convert(v);
        match converted {
            Some(converted) => Ok(converted),
            None => Err(format!("argument '{argument_name}' of message '{message_name}' is not {}", Self::type_description())),
        }
    }
    fn convert(v: &nvim_rs::Value) -> Option<Self>;
    fn type_description() -> String;
}

impl<T: FromNvimValue> FromNvimValue for Vec<T> {
    fn convert(v: &nvim_rs::Value) -> Option<Self> {
        v.as_array()?.into_iter().map(|v| T::convert(v)).collect::<Option<Vec<_>>>()
    }

    fn type_description() -> String {
        format!("array of {}", T::type_description())
    }
}
impl<T: FromNvimValue> FromNvimValue for Option<T> {
    fn convert(v: &nvim_rs::Value) -> Option<Self> {
        if v.is_nil() {
            Some(None)
        } else {
            T::convert(v).map(Some)
        }
    }

    fn type_description() -> String {
        format!("{} or nil", T::type_description())
    }
}
impl FromNvimValue for bool {
    fn convert(v: &nvim_rs::Value) -> Option<bool> {
        v.as_bool()
    }
    fn type_description() -> String {
        "a bool".to_string()
    }
}
impl FromNvimValue for String {
    fn convert(v: &nvim_rs::Value) -> Option<String> {
        v.as_str().map(|s| s.to_string())
    }
    fn type_description() -> String {
        "a string".to_string()
    }
}

macro_rules! messages {
    ($vis:vis enum $message_enum_name:ident { $($message_name_pascal:ident, $message_name_snake:ident, { $($field_name:ident: $field_ty:ty),* $(,)? }),* $(,)? }) => {
        $vis enum $message_enum_name {
            $(
                $message_name_pascal { $($field_name: $field_ty),* },
            )+
            Invalid(String),
        }

        impl $message_enum_name {
            #[inline]
            pub fn parse(message: String, args: Vec<nvim_rs::Value>) -> $message_enum_name {
                match &*message {
                    $(
                        stringify!($message_name_snake) => {
                            #[allow(unused_mut, unused_variables)]
                            let mut arg_iter = args.iter().chain(std::iter::repeat(&nvim_rs::Value::Nil));

                            #[allow(clippy::redundant_closure_call)]
                            let result = (|| {
                                $(
                                    let $field_name = <$field_ty as FromNvimValue>::from(&message, stringify!($field_name), &arg_iter.next().expect("infinite iterator should always have next value"))?;
                                )*

                                Ok::<_, String>($message_enum_name::$message_name_pascal { $( $field_name ),* })
                            })();

                            match result {
                                Ok(res) => res,
                                Err(err) => Message::Invalid(err),
                            }
                        }
                    )+
                    _ => Message::Invalid(format!("unknown message '{message}' with params {args:?}")),
                }
            }
        }
    };
}

messages! {
    pub enum Message {
        NewNote, new_note, { directory: Vec<String>, focus: bool },
        OpenIndex, open_index, {}, // TODO: configurable index file name?
        DeleteNote, delete_note, {},
        NewNoteAndInsertLink, new_note_and_insert_link, {},
        OpenTagIndex, open_tag_index, {},
        FollowLink, follow_link, {},
        InsertLinkAtCursor, insert_link_at_cursor, { link_to_directories: Vec<String>, link_to_id: String, link_text: Option<String> },
        InsertLinkAtCursorOrCreate, insert_link_at_cursor_or_create, { link_to_directories: Vec<String>, link_to_id: Option<String>, link_text: Option<String> },
    }
}
