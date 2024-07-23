use std::path::PathBuf;

use chrono::{Local, Utc};
use futures_io::AsyncWrite;
use nvim_rs::{compat::tokio::Compat, create, error::CallError, Handler, Neovim, Value};

enum Error {
    NvimCallError(CallError),
    Str(String),
}

impl From<CallError> for Error {
    fn from(v: CallError) -> Self {
        Self::NvimCallError(v)
    }
}
impl From<Box<CallError>> for Error {
    fn from(v: Box<CallError>) -> Self {
        Self::NvimCallError(*v)
    }
}
impl From<String> for Error {
    fn from(v: String) -> Self {
        Self::Str(v)
    }
}

enum Message {
    NewNote { directory: String, focus: bool },
    Error(String),
}

impl Message {
    fn parse(method: String, args: Vec<Value>) -> Message {
        fn to_string(method_name: &str, argument_name: &str, v: &Value) -> Result<String, String> {
            v.as_str().ok_or_else(move || format!("argument '{argument_name}' of method '{method_name}' is not a string")).map(|s| s.to_string())
        }
        fn to_bool(method_name: &str, argument_name: &str, v: &Value) -> Result<bool, String> {
            v.as_bool().ok_or_else(move || format!("argument '{argument_name}' of method '{method_name}' is not a bool"))
        }

        macro_rules! parse_params {
            ($n_params:literal, $(($pname:ident, $parse_function:expr)),+, $method_name:ident, $params:ident, || $result:expr) => {
                if $params.len() == $n_params {
                    #[allow(unused_assignments)]

                    let result = (|| {
                    let mut param_index = 0;
                        $(
                            let $pname = $parse_function(&$method_name, stringify!($pname), &$params[param_index])?;
                            param_index += 1;
                        )+

                        Ok::<_, String>($result)
                    })();

                    match result {
                        Ok(res) => res,
                        Err(err) => Message::Error(err),
                    }
                } else {
                    Message::Error(format!("method '{}' needs {} parameters", $method_name, $n_params))
                }
            };
        }

        match &*method {
            "new_note" => parse_params!(2, (directory, to_string), (focus, to_bool), method, args, || Message::NewNote { directory, focus }),
            _ => Message::Error(format!("unknown method '{method}' with params {args:?}")),
        }
    }
}

#[derive(Clone)]
struct Config {
    home_path: PathBuf,
    note_id_timestamp_format: String,
    date_format: String,
    time_format: String,
}
impl Config {
    fn parse_from_args() -> Config {
        let mut args = std::env::args().skip(1);
        Config {
            home_path: args.next().expect("first argument should be wiki home path").into(),
            note_id_timestamp_format: args.next().expect("second argument should be note id timestamp format"),
            date_format: args.next().expect("third argument should be date format"),
            time_format: args.next().expect("fourth argument should be time format"),
        }
    }
}

#[derive(Clone)]
struct WikiPlugin {
    config: Config,
}

impl WikiPlugin {
    async fn new_note<W: Send + Unpin + AsyncWrite>(&self, nvim: &mut Neovim<W>, directory: String, focus: bool) -> Result<(), Error> {
        let now = Local::now();

        let title = nvim.eval(r#"input("note name: ")"#).await?;
        let buf_path = {
            let mut p = self.config.home_path.join(directory);
            p.push(now.format(&self.config.note_id_timestamp_format).to_string());
            p.set_extension("md");
            p
        };
        let buf_contents = [
            "---".to_string(),
            format!("title: {title}"),
            format!("date: {}", now.format(&self.config.date_format)),
            format!("time: {}", now.format(&self.config.time_format)),
            "tags:".to_string(),
            "---".to_string(),
        ]
        .to_vec();

        let buf = nvim.create_buf(true, false).await?;
        buf.set_name(buf_path.to_str().ok_or_else(|| format!("invalid buf path {buf_path:?}"))?).await?;
        buf.set_lines(0, 0, true, buf_contents).await?;
        buf.set_option("filtype", "zet".into()).await?;

        if focus {
            nvim.set_current_buf(&buf).await?;
        }

        Ok(())
    }

    async fn do_message<W: Send + Unpin + AsyncWrite>(&self, nvim: &mut Neovim<W>, message: Message) -> Result<(), Error> {
        match message {
            Message::NewNote { directory, focus } => self.new_note(nvim, directory, focus).await,
            Message::Error(e) => Err(e.into()),
        }
    }
}

#[async_trait::async_trait]
impl Handler for WikiPlugin {
    type Writer = Compat<tokio::fs::File>;

    async fn handle_notify(&self, name: String, args: Vec<Value>, mut nvim: Neovim<Compat<tokio::fs::File>>) {
        let message = Message::parse(name, args);
        if let Err(e) = self.do_message(&mut nvim, message).await {
            let e_text = match e {
                Error::NvimCallError(e) => e.to_string(),
                Error::Str(s) => s,
            };
            log::error!("error handling notify event: {e_text}");
            let _ = nvim.err_writeln(&format!("error handling notify event: {e_text}")).await;
        }
    }
}

#[tokio::main]
async fn main() {
    flexi_logger::Logger::try_with_env().expect("could not initialize logger").log_to_file(flexi_logger::FileSpec::default()).start().expect("could not initialize logger");

    let plugin = WikiPlugin { config: Config::parse_from_args() };

    let (nvim, io_handler) = create::tokio::new_parent(plugin).await.unwrap();
    match io_handler.await {
        Err(joinerr) => {
            log::error!("error joining io loop: '{joinerr}'")
        }
        Ok(Err(err)) => {
            if !err.is_reader_error() {
                let _ = nvim.err_writeln(&format!("error: '{}'", err)).await;
                log::error!("error: '{}'", err);
            }

            if !err.is_channel_closed() {
                log::error!("error: '{}'", err);

                use std::error::Error;
                let mut source = err.source();

                while let Some(e) = source {
                    log::error!("caused by: '{}'", e);
                    source = e.source();
                }
            }
        }

        Ok(Ok(())) => {}
    }
}
