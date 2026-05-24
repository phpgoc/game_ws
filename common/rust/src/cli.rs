use clap::Parser;

#[derive(Debug, Clone, Parser)]
#[command(about = "Game WS server")]
pub struct BindCli {
    #[arg(long)]
    pub host: Option<String>,
    #[arg(long)]
    pub port: Option<u16>,
}

pub fn parse_bind_cli() -> BindCli {
    BindCli::parse()
}
