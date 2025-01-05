use crate::table;
use bollard::{
    container::ListContainersOptions,
    errors,
    secret::{ContainerSummary, Port, PortTypeEnum},
    Docker,
};
use serde::Serialize;

#[derive(clap::Parser)]
struct Args {
    /// Show all containers. Not only the running ones.
    #[arg(short, long)]
    all: bool,
}

#[derive(clap::Subcommand)]
enum Commands {
    /// Pretty print the `docker ps` output. Long branch mappings are folded up nicely. Etc..
    Ps(Args),
}

#[derive(clap::Parser)]
pub(crate) struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

struct DockerApi {
    rt: tokio::runtime::Runtime,
    docker: bollard::Docker,
}

impl DockerApi {
    async fn list_containers(&self, all: bool) -> Result<Vec<ContainerSummary>, errors::Error> {
        let options = ListContainersOptions::<String> {
            all,
            size: true,
            ..Default::default()
        };
        return self.docker.list_containers(options.into()).await;
    }

    fn new() -> Result<Self, bollard::errors::Error> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Tokio?");
        let docker = Docker::connect_with_local_defaults()?;
        Ok(Self { rt, docker })
    }
}

const ID_LENGTH: usize = 12;
const IMAGE_ID_PREFIX: &str = "sha256:";

#[derive(Default, Serialize)]
#[serde(rename_all = "UPPERCASE")]
struct PsRow {
    id: String,
    names: String,
    ports: String,
    status: String,
    size: String,
    image: String,
    nets: String,
}

impl<'a> table::TableRow<'a> for PsRow {}

fn format_port(port: &Port) -> String {
    let private = port.private_port;
    let typ = match port.typ.unwrap_or(PortTypeEnum::EMPTY) {
        PortTypeEnum::EMPTY => "",
        PortTypeEnum::TCP => "/tcp",
        PortTypeEnum::UDP => "/udp",
        PortTypeEnum::SCTP => "/sctp",
    };
    if let (Some(ip), Some(public)) = (&port.ip, &port.public_port) {
        let ip_format = match ip.as_str() {
            "::" => "[::]:".into(),
            ip => format!("{ip}:"),
        };
        return format!("{ip_format}{public}->{private}{typ}");
    } else {
        return format!("{private}{typ}");
    }
}

impl Cli {
    fn ps(&self, cmd: &DockerApi, args: &Args) -> Result<(), bollard::errors::Error> {
        let containers = cmd.rt.block_on(cmd.list_containers(args.all))?;
        let mut rows: Vec<_> = Vec::with_capacity(containers.len());
        for container in containers {
            let mut row = PsRow::default();
            if let Some(id) = container.id {
                row.id = id.chars().take(ID_LENGTH).collect();
            }
            if let Some(names) = container.names {
                row.names = names.join(", ")
            }
            let mut collapsed: Vec<_> = vec![];
            if let Some(ports) = container.ports {
                for (i, port) in ports.iter().enumerate() {
                    if i == 0 {
                        row.ports = format_port(&port);
                        continue; /*UGHH*/
                    }
                    let mut row = PsRow::default();
                    row.ports = format_port(&port);
                    collapsed.push(row);
                }
            }
            if let Some(status) = container.status {
                row.status = status;
            }
            if let Some(size) = container.size_root_fs {
                row.size = size.to_string();
            }
            if let Some(image) = container.image {
                let id: String = container
                    .image_id
                    .expect("image should have image id")
                    .chars()
                    .skip(IMAGE_ID_PREFIX.len())
                    .take(ID_LENGTH)
                    .collect();
                row.image = format!("{}/{}", image, id);
            }
            if let Some(net) = container.network_settings {
                if let Some(nets) = net.networks {
                    let nets: Vec<_> = nets.into_keys().collect();
                    row.nets = nets.join(", ");
                }
            }
            rows.push(row);
            rows.extend(collapsed);
        }
        println!("{}", table::Renderer::default().to_string(&rows));
        Ok(())
    }

    pub(crate) fn run(&self) -> Result<(), bollard::errors::Error> {
        let api = DockerApi::new()?;
        if let Some(command) = &self.command {
            match command {
                Commands::Ps(args) => self.ps(&api, args)?,
            }
        }
        Ok(())
    }
}
