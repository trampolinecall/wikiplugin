use crate::plugin::{Config, WikiPlugin};

mod connection;
mod error;
mod plugin;

#[tokio::main]
async fn main() {
    flexi_logger::Logger::try_with_env()
        .expect("could not initialize logger")
        .log_to_file(flexi_logger::FileSpec::default())
        .start()
        .expect("could not initialize logger");

    let plugin = WikiPlugin { config: Config::parse_from_args() };

    connection::make_connection(plugin).await;
}
