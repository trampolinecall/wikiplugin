use nvim_oxi::api;

use std::error::Error;

// TODO: move this function to an error module
pub fn print_error(err: &dyn Error) {
    let mut err_str = format!("error: {err}\n");

    let mut source = err.source();
    while let Some(e) = source {
        use std::fmt::Write;
        writeln!(err_str, "caused by '{e}'").expect("writing to a String cannot fail");
        source = e.source();
    }

    api::err_writeln(&err_str);
    log::error!("{}", err_str);
}
