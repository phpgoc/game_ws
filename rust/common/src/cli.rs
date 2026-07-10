use clap::Parser;

#[derive(Debug, Clone, Parser)]
#[command(about = "Game WS server")]
pub(crate) struct BindCli {
    #[arg(long)]
    pub host: Option<String>,
    #[arg(long)]
    pub port: Option<u16>,
}

pub(crate) fn parse_bind_cli() -> BindCli {
    BindCli::parse()
}
