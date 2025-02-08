use nvim_rs::{compat::tokio::Compat, Neovim};

use std::error::Error;

use crate::plugin::WikiPlugin;

#[derive(Debug)]
pub enum ConnectionError {
    LoopError(Box<nvim_rs::error::LoopError>),
    JoinError(tokio::task::JoinError),
    IoError(std::io::Error),
    CallError(Box<nvim_rs::error::CallError>),
    InvalidLuaToRustCast(&'static str),
}

impl From<Box<nvim_rs::error::CallError>> for ConnectionError {
    fn from(v: Box<nvim_rs::error::CallError>) -> Self {
        Self::CallError(v)
    }
}

impl From<std::io::Error> for ConnectionError {
    fn from(v: std::io::Error) -> Self {
        Self::IoError(v)
    }
}

impl From<tokio::task::JoinError> for ConnectionError {
    fn from(v: tokio::task::JoinError) -> Self {
        Self::JoinError(v)
    }
}

impl From<Box<nvim_rs::error::LoopError>> for ConnectionError {
    fn from(v: Box<nvim_rs::error::LoopError>) -> Self {
        Self::LoopError(v)
    }
}

impl std::fmt::Display for ConnectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectionError::LoopError(loop_error) => loop_error.fmt(f),
            ConnectionError::JoinError(join_error) => join_error.fmt(f),
            ConnectionError::IoError(error) => error.fmt(f),
            ConnectionError::CallError(call_error) => call_error.fmt(f),
            ConnectionError::InvalidLuaToRustCast(msg) => write!(f, "{msg}"),
        }
    }
}

pub async fn make_connection(plugin: WikiPlugin) {
    let (mut nvim, io_handler) = match nvim_rs::create::tokio::new_parent(plugin).await {
        Ok(res) => res,
        Err(e) => {
            print_error_no_nvim(&e).await;
            return;
        }
    };

    match io_handler.await {
        Err(joinerr) => {
            print_error_no_nvim(&joinerr).await;
        }
        Ok(Err(err)) => {
            print_error(&mut nvim, &err).await;
        }

        Ok(Ok(())) => {}
    }
}

pub async fn print_error(nvim: &mut Neovim<Compat<tokio::fs::File>>, err: &(dyn Error + Sync + Send)) {
    print_error_helper(Some(nvim), err).await
}
pub async fn print_error_no_nvim(err: &(dyn Error + Sync + Send)) {
    print_error_helper(None, err).await
}

async fn print_error_helper(nvim: Option<&mut Neovim<Compat<tokio::fs::File>>>, err: &(dyn Error + Sync + Send)) {
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
