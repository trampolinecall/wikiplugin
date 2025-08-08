use nvim_oxi::api;

use std::error::Error;

macro_rules! error_union {
    ($vis:vis enum $name:ident { $( $variant_name:ident($error_type:ty) ),* $(,)? }) => {
        #[derive(Debug)]
        $vis enum $name {
            $(
                $variant_name($error_type),
            )*
        }

        impl std::error::Error for $name {}
        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self {
                    $(
                        $name::$variant_name(e) => e.fmt(f),
                    )*
                }

            }
        }

        $(
            impl From<$error_type> for $name {
                fn from(v: $error_type) -> $name {
                    $name::$variant_name(v)
                }
            }
        )*
    };
}

macro_rules! convert_error_union {
    ($ty1:ident => $ty2:ident { $($var1:ident => $var2:ident),* $(,)? }) => {
        impl From<$ty1> for $ty2 {
            fn from(v: $ty1) -> $ty2 {
                match v {
                    $(
                        $ty1::$var1(e) => $ty2::$var2(e),
                    )*
                }
            }
        }
    };
}

pub fn print_error(err: &dyn Error) {
    let mut err_str = format!("error: {err}\n");

    let mut source = err.source();
    while let Some(e) = source {
        use std::fmt::Write;
        writeln!(err_str, "caused by '{e}'").expect("writing to a String cannot fail");
        source = e.source();
    }

    api::err_writeln(&err_str);
    log::error!("{err_str}");
}
