use nvim_rs::{compat::tokio::Compat, Neovim};

use crate::{error::Error, plugin::WikiPlugin};

pub async fn make_connection(plugin: WikiPlugin) {
    let (mut nvim, io_handler) = nvim_rs::create::tokio::new_parent(plugin).await.unwrap();
    match io_handler.await {
        Err(joinerr) => {
            print_error_no_nvim(format!("error joining io loop: '{joinerr}'").into()).await;
        }
        Ok(Err(err)) => {
            print_error(&mut nvim, Box::new(err)).await;
        }

        Ok(Ok(())) => {}
    }
}

pub async fn print_error(nvim: &mut Neovim<Compat<tokio::fs::File>>, err: Error) {
    print_error_helper(Some(nvim), err).await
}
pub async fn print_error_no_nvim(err: Error) {
    print_error_helper(None, err).await
}

async fn print_error_helper(nvim: Option<&mut Neovim<Compat<tokio::fs::File>>>, err: Error) {
    let mut err_str = format!("error: {err}\n");

    let mut source = err.source();
    while let Some(e) = source {
        use std::fmt::Write;
        writeln!(err_str, "caused by '{e}'").expect("writing to a String cannot fail");
        source = e.source();
    }

    if let Some(nvim) = nvim {
        let _ = nvim.err_write(&err_str).await;
    }
    log::error!("{}", err_str);
}
