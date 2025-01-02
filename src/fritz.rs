use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};
use std::{env, fs, path::PathBuf};

use crate::table;

#[derive(clap::Parser)]
struct Args {}

#[derive(clap::Subcommand)]
enum Commands {
    /// Initialize the config for the tool automatically.
    Init(Args),
    /// Display debugging information about the FritzBox.
    Info(Args),
    /// Reboot the device instantly.
    Reboot(Args),
    /// Reconnect the device, usually with a new IP.
    Reconnect(Args),
    /// List all known devices and device info.
    Devices(Args),
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct Session {
    #[serde(rename = "SID")]
    sid: String,
    #[serde(rename = "Challenge")]
    challenge: String,
    #[serde(rename = "BlockTime")]
    block_time: i64,
}

#[derive(Debug, Deserialize, Serialize)]
struct Config {
    base_url: String,
    username: String,
    password: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct ConnectionInfo {
    #[serde(rename = "led")]
    state: String,
    up: String,
    down: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct OsInfo {
    #[serde(rename = "Productname")]
    product_name: String,
    #[serde(rename = "isUpdateAvail")]
    is_update_available: bool,
    #[serde(rename = "nspver")]
    version: String,
    #[serde(rename = "fb_name")]
    name: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct OverviewData {
    #[serde(rename = "fritzos")]
    os: OsInfo,
    internet: ConnectionInfo,
    dsl: ConnectionInfo,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Overview {
    pid: String,
    data: OverviewData,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct IpV4 {
    ip: String,
    lastused: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Device {
    #[serde(rename = "UID")]
    uid: String,
    classes: Option<String>,
    #[serde(rename = "ipv4")]
    ipv4: Option<IpV4>,
    #[serde(rename = "isTrusted")]
    is_trusted: Option<bool>,
    mac: Option<String>,
    model: Option<String>,
    name: Option<String>,
    state: Option<String>,
    #[serde(rename = "type")]
    typ: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct DeviceData {
    passive: Option<Vec<Device>>,
    active: Option<Vec<Device>>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Devices {
    pid: String,
    data: Option<DeviceData>,
}

impl Devices {
    fn devices(self) -> Option<impl Iterator<Item = Device>> {
        let d = self.data?;
        let active = d.active.into_iter().flatten();
        let passive = d.passive.into_iter().flatten();
        return active.chain(passive).into();
    }
}

#[derive(Debug)]
struct FritzApi {
    config: Config,
    session: Session,
    client: reqwest::blocking::Client,
}

type AnyError<T> = Result<T, Box<dyn std::error::Error>>;

macro_rules! form_data {
      ($( $pairs:expr ),*) => {{
        use form_urlencoded::Serializer;
        let mut se = Serializer::new(String::new());
        {$(for pair in $pairs {
            se.append_pair(pair.0, pair.1);
        })*}
        se.finish()
    }};
}

impl FritzApi {
    fn login(&self) -> AnyError<Session> {
        let challenge_response = self
            .client
            .get(format!("{base}/login_sid.lua", base = self.config.base_url))
            .send()?
            .text()?;
        let info: Session = serde_xml_rs::from_str(&challenge_response)?;

        let mut buf: Vec<u8> = vec![];
        for char in (format!("{}-{}", info.challenge, self.config.password)).encode_utf16() {
            let ch = match char {
                ch if ch > 255 => 0x2e,
                other => other,
            };
            use std::io::Write;
            Write::write(&mut buf, &ch.to_le_bytes())?;
        }

        let mut hasher = Md5::new();
        hasher.update(&buf);
        let sum = hasher
            .finalize()
            .to_vec()
            .iter()
            .map(|byte| format!("{:02x}", byte))
            .collect::<String>();

        let auth_response = self
            .client
            .post(format!("{base}/login_sid.lua", base = self.config.base_url))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(form_data!(&[
                ("username", &self.config.username),
                ("response", &format!("{}-{}", info.challenge, sum)),
            ]))
            .send()?
            .text()?;

        // TODO: reject with error if sids of challenge response and auth_response match.
        let session: Session = serde_xml_rs::from_str(&auth_response)?;
        Ok(session)
    }

    fn authenticated(config: Config) -> AnyError<Self> {
        let client = reqwest::blocking::Client::new();
        let mut api = Self {
            client,
            config,
            session: Session::default(),
        };
        let session = api.login()?;
        api.session = session;
        Ok(api)
    }

    fn query_overview(&self) -> AnyError<Overview> {
        println!("Fetching...");
        let res = self
            .client
            .post(format!("{base}/data.lua", base = self.config.base_url))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(form_data!(&[
                ("sid", self.session.sid.as_str()),
                ("page", "overview"),
            ]))
            .send()?
            .text()?;
        let overview: Overview = serde_json::from_str(&res)?;
        Ok(overview)
    }

    fn query_devices(&self) -> AnyError<Devices> {
        println!("Fetching...");
        let res = self
            .client
            .post(format!("{base}/data.lua", base = self.config.base_url))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(form_data!(&[
                ("sid", self.session.sid.as_str()),
                ("page", "netDev"),
                ("xhrId", "all"),
            ]))
            .send()?
            .text()?;
        let devices: Devices = serde_json::from_str(&res)?;
        return Ok(devices);
    }

    fn reboot(&self) -> AnyError<bool> {
        let res = self
            .client
            .post(format!("{base}/data.lua", base = self.config.base_url))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(form_data![&[
                ("sid", self.session.sid.as_str()),
                ("page", "reboot"),
                ("reboot", "0"),
            ]])
            .send()?;

        let result: serde_json::Value = serde_json::from_str(&res.text()?)?;
        let status = result
            .pointer("data/reboot")
            .and_then(serde_json::Value::as_str);

        if let Some("ok") = status {
            self.client
                .post(format!("{base}/reboot.lua", base = self.config.base_url))
                .header("Content-Type", "application/x-www-form-urlencoded")
                .body(form_data!(&[
                    ("sid", self.session.sid.as_str()),
                    ("ajax", "1"),
                ]))
                .send()?;
            return Ok(true);
        }
        return Ok(false);
    }

    fn disconnect(&self) -> reqwest::Result<serde_json::Value> {
        self.client
            .get(format!(
                "{base}/internet/inetstat_monitor.lua",
                base = self.config.base_url
            ))
            .query(&[
                ("sid", self.session.sid.as_str()),
                ("myXhr", "1"),
                ("action", "disconnect"),
            ])
            .send()?;

        Ok(serde_json::Value::Null)
    }

    fn connect(&self) -> reqwest::Result<serde_json::Value> {
        self.client
            .get(format!(
                "{base}/internet/inetstat_monitor.lua",
                base = self.config.base_url
            ))
            .query(&[
                ("sid", self.session.sid.as_str()),
                ("myXhr", "1"),
                ("action", "connect"),
            ])
            .send()?;

        Ok(serde_json::Value::Null)
    }

    fn reconnect(&self) -> reqwest::Result<serde_json::Value> {
        self.disconnect()?;
        self.connect()?;
        Ok(serde_json::Value::Null)
    }
}

#[derive(clap::Parser)]
pub(crate) struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Default, Serialize)]
#[serde(rename_all = "UPPERCASE")]
struct DevicesRow {
    name: String,
    ip: String,
    lastused: String,
    #[serde(rename = "type")]
    typ: String,
    model: String,
    uid: String,
    trusted: String,
    state: String,
}

impl<'a> table::TableRow<'a> for DevicesRow {}

#[derive(Default, Serialize)]
#[serde(rename_all = "UPPERCASE")]
struct OverviewRow {
    model: String,
    version: String,
    name: String,
    update_available: String,
    dsl_status: String,
    inet_status: String,
}

impl<'a> table::TableRow<'a> for OverviewRow {}

impl Cli {
    fn info(&self, api: &FritzApi, _args: &Args) -> AnyError<()> {
        let overview = api.query_overview()?;

        let os = overview.data.os;
        let mut row = OverviewRow::default();
        row.model = os.product_name;
        row.version = os.version;
        row.name = os.name;
        row.update_available = (if os.is_update_available { "Yes" } else { "No" }).to_string();

        let dsl = overview.data.dsl;
        row.dsl_status = format!(
            "{up} / {down} ({state})",
            state = dsl.state,
            up = dsl.up,
            down = dsl.down
        );

        let inet = overview.data.internet;
        row.inet_status = format!(
            "{up} / {down} ({state})",
            state = inet.state,
            up = inet.up,
            down = inet.down
        );

        println!("{}", table::Renderer::default().to_string(&[row]));
        Ok(())
    }

    fn reboot(&self, api: &FritzApi, _args: &Args) -> AnyError<()> {
        let ok = api.reboot()?;
        println!(
            "Reboot status: {}",
            match ok {
                true => "Rebooting... This can take some time!! (5-10m)",
                _ => "No Reboot!",
            }
        );
        Ok(())
    }

    fn reconnect(&self, api: &FritzApi, _args: &Args) -> reqwest::Result<()> {
        api.reconnect()?;
        println!("Heads up! This can take up to 30s to take full effect..");
        Ok(())
    }

    fn devices(&self, api: &FritzApi, _args: &Args) -> AnyError<()> {
        let data = api.query_devices()?;
        let mut rows = vec![];
        if let Some(devices) = data.devices() {
            for device in devices {
                let mut row = DevicesRow::default();
                if let Some(name) = device.name {
                    row.name = name;
                }
                if let Some(ipv4) = device.ipv4 {
                    row.ip = ipv4.ip;
                    row.lastused = ipv4.lastused.unwrap_or_default();
                }
                if let Some(typ) = device.typ {
                    row.typ = typ;
                }
                if let Some(model) = device.model {
                    row.model = model;
                }
                row.uid = device.uid;
                if let Some(trusted) = device.is_trusted {
                    if trusted {
                        row.trusted = "Yes".to_string();
                    }
                }
                if let Some(state) = device.state {
                    row.state = state;
                }
                rows.push(row);
            }
        }
        println!("{}", table::Renderer::default().to_string(&rows));
        Ok(())
    }

    pub(crate) fn run(&self) -> AnyError<()> {
        if let Some(command) = &self.command {
            let home = PathBuf::from(
                env::var_os("HOME").expect("should be able to get `$HOME` from process."),
            );
            let config_path = home.join(".config/fritz/config.json");
            let config = serde_json::from_reader(fs::File::open(config_path)?)?;
            let api = FritzApi::authenticated(config)?;
            match command {
                Commands::Info(args) => self.info(&api, args)?,
                Commands::Reboot(args) => self.reboot(&api, args)?,
                Commands::Reconnect(args) => self.reconnect(&api, args)?,
                Commands::Devices(args) => self.devices(&api, args)?,
                _ => unimplemented!("Command not implemented yet!"),
            }
        }
        Ok(())
    }
}

// TODO: save sid with expire date to session.json and reuse?
// TODO: xdg location for cache and config files.
