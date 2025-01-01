/*** UNDER CONSTRUCTION ***/
/*** WORK IN PROGGERS   ***/
use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};
use std::fs;

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

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
struct IpV4 {
    ip: String,
    lastused: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
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

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
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

impl FritzApi {
    fn login(&self) -> reqwest::Result<Session> {
        let challenge_response = self
            .client
            .get(format!("{base}/login_sid.lua", base = self.config.base_url))
            .send()?
            .text()?;
        let info: Session =
            serde_xml_rs::from_str(&challenge_response).expect("Should get the session info");

        let mut buf: Vec<u8> = vec![];
        for char in (format!("{}-{}", info.challenge, self.config.password)).encode_utf16() {
            let ch = match char {
                ch if ch > 255 => 0x2e,
                other => other,
            };
            use std::io::Write;
            Write::write(&mut buf, &ch.to_le_bytes()).expect("write should work");
        }

        let mut hasher = Md5::new();
        hasher.update(&buf);
        let sum = hasher
            .finalize()
            .to_vec()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>();

        let payload = form_urlencoded::Serializer::new(String::new())
            .append_pair("username", self.config.username.as_ref())
            .append_pair("response", &format!("{}-{}", info.challenge, sum))
            .finish();

        let auth_response = self
            .client
            .post(format!("{base}/login_sid.lua", base = self.config.base_url))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(payload)
            .send()?
            .text()?;

        // TODO: reject with error if sids of challenge response and auth_response match.
        let session: Session =
            serde_xml_rs::from_str(&auth_response).expect("Should get the session info");
        Ok(session)
    }

    fn authenticated(config: Config) -> reqwest::Result<Self> {
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

    fn query_overview(&self) -> reqwest::Result<serde_json::Value> {
        let payload = form_urlencoded::Serializer::new(String::new())
            .append_pair("sid", self.session.sid.as_str())
            .append_pair("page", "overview")
            .finish();

        let res = self
            .client
            .post(format!("{base}/data.lua", base = self.config.base_url))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(payload)
            .send()?
            .text()?;

        Ok(serde_json::from_str(&res).unwrap_or_default())
    }

    fn query_devices(&self) -> reqwest::Result<Devices> {
        let payload = form_urlencoded::Serializer::new(String::new())
            .append_pair("sid", self.session.sid.as_str())
            .append_pair("page", "netDev")
            .append_pair("xhrId", "all")
            .finish();

        let res = self
            .client
            .post(format!("{base}/data.lua", base = self.config.base_url))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(payload)
            .send()?
            .text()?;
        return Ok(serde_json::from_str(&res).unwrap_or_default());
    }

    fn reboot(&self) -> reqwest::Result<serde_json::Value> {
        let payload = form_urlencoded::Serializer::new(String::new())
            .append_pair("sid", self.session.sid.as_str())
            .append_pair("page", "reboot")
            .append_pair("reboot", "0")
            .finish();

        let res = self
            .client
            .post(format!("{base}/data.lua", base = self.config.base_url))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(payload)
            .send()?;

        let result: serde_json::Value = serde_json::from_str(&res.text()?).unwrap();

        let payload = form_urlencoded::Serializer::new(String::new())
            .append_pair("sid", self.session.sid.as_str())
            .append_pair("ajax", "1")
            .finish();

        self.client
            .post(format!("{base}/reboot.lua", base = self.config.base_url))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(payload)
            .send()?;

        Ok(result)
    }

    fn disconnect(&self) -> reqwest::Result<serde_json::Value> {
        let query_data = [
            ("sid", self.session.sid.as_str()),
            ("myXhr", "1"),
            ("action", "disconnect"),
        ];

        self.client
            .get(format!(
                "{base}/internet/inetstat_monitor.lua",
                base = self.config.base_url
            ))
            .query(&query_data)
            .send()?;

        Ok(serde_json::Value::Null)
    }

    fn connect(&self) -> reqwest::Result<serde_json::Value> {
        let query_data = [
            ("sid", self.session.sid.as_str()),
            ("myXhr", "1"),
            ("action", "connect"),
        ];

        self.client
            .get(format!(
                "{base}/internet/inetstat_monitor.lua",
                base = self.config.base_url
            ))
            .query(&query_data)
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
    #[serde(rename = "type")]
    typ: String,
    model: String,
    uid: String,
    trusted: bool,
    state: String,
}

impl<'a> table::TableRow<'a> for DevicesRow {}

impl Cli {
    fn info(&self, api: &FritzApi, _args: &Args) -> reqwest::Result<()> {
        let data = api.query_overview()?;
        println!("{}", data);
        // TODO: tabular data lister for most of it using using table crate
        Ok(())
    }
    fn reboot(&self, api: &FritzApi, _args: &Args) -> reqwest::Result<()> {
        let result = api.reboot()?;
        if result.is_object() {
            println!("Reboot status: {}", result.get("data").unwrap());
        }
        Ok(())
    }
    fn reconnect(&self, api: &FritzApi, _args: &Args) -> reqwest::Result<()> {
        api.reconnect()?;
        println!("Heads up! This can take up to 30s to take full effect..");
        Ok(())
    }
    fn devices(&self, api: &FritzApi, _args: &Args) -> reqwest::Result<()> {
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
                }
                if let Some(typ) = device.typ {
                    row.typ = typ;
                }
                if let Some(model) = device.model {
                    row.model = model;
                }
                row.uid = device.uid;
                if let Some(trusted) = device.is_trusted {
                    row.trusted = trusted;
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
    pub(crate) fn run(&self) -> reqwest::Result<()> {
        if let Some(command) = &self.command {
            let file = fs::File::open("./config.json").unwrap();
            let config: Config = serde_json::from_reader(file).unwrap();
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

// TODO: clean up unwraps/expects and fix up error handling
// TODO: save sid with expire date to session.json and reuse?
// TODO: xgd location for cache and config files.
