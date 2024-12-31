use clap::Parser;
mod docker;
mod fritz;
mod table;

#[derive(clap::Subcommand)]
enum Commands {
    Docker(docker::Cli),
    Fritz(fritz::Cli),
}

#[derive(clap::Parser)]
#[command(version, about, long_about=None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Cli::parse();
    if let Some(command) = args.command {
        match command {
            Commands::Docker(cli) => cli.run().map_err(|err| format!("{err}"))?,
            Commands::Fritz(cli) => cli.run().map_err(|err| format!("{err}"))?,
        }
    }
    Ok(())
}
