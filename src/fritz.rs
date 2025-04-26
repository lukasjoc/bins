use crate::table;
use core::fmt;
use md5::{digest::FixedOutputReset, Digest, Md5};
use reqwest as rw;
use serde::{Deserialize, Serialize};
use std::env;

#[derive(clap::Parser)]
struct Args {}

#[derive(clap::Subcommand)]
enum Commands {
    /// Display debugging information about the FritzBox.
    Info(Args),
    /// Reboot the device instantly.
    Reboot(Args),
    /// Reconnect the device, usually with a new IP.
    Reconnect(Args),
    /// List all known devices and device info.
    Devices(Args),
}

#[derive(Debug, Serialize, Deserialize)]
struct Session {
    #[serde(rename = "SID")]
    sid: String,
    #[serde(rename = "Challenge")]
    challenge: String,
    #[serde(rename = "BlockTime")]
    block_time: i64,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            sid: "0000000000000000".into(),
            challenge: String::default(),
            block_time: i64::default(),
        }
    }
}

impl Session {
    fn is_default_sid(&self) -> bool {
        return self.sid == Self::default().sid;
    }
}

#[derive(Debug, Deserialize, Serialize, Default)]
struct Config<'a> {
    base_url: &'a str,
    username: &'a str,
    password: &'a str,
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

// TODO: I dont i need this trait anymore.
trait FritzApiFunctions {
    /// Optain a new session with a given user config.
    fn login(&mut self) -> AnyError<&Session>;
    /// Query the overview data.
    fn overview(&self) -> AnyError<Overview>;
    /// Query the devices data.
    fn devices(&self) -> AnyError<Devices>;
    // Reboot the device.
    fn reboot(&self) -> AnyError<bool>;
    // Connect the device.
    fn connect(&self) -> rw::Result<serde_json::Value>;
    // Disconnect the device.
    fn disconnect(&self) -> rw::Result<serde_json::Value>;
}

#[derive(Debug)]
struct FritzClient<'a> {
    config: Config<'a>,
    session: Session,
    client: rw::blocking::Client,
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

#[derive(Debug)]
struct LoginError;
impl LoginError {
    fn boxed() -> Box<Self> {
        Box::new(Self)
    }
}

impl fmt::Display for LoginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", "Login was not successful!")
    }
}

impl std::error::Error for LoginError {}

impl<'a> FritzClient<'a> {
    fn new_with_config(config: Config<'a>) -> Self {
        Self {
            client: rw::blocking::Client::new(),
            config,
            session: Session::default(),
        }
    }
}

impl<'a> FritzApiFunctions for FritzClient<'a> {
    fn login(&mut self) -> AnyError<&Session> {
        let path = format!("{base}/login_sid.lua", base = self.config.base_url);
        let challenge_data_raw = self.client.get(path).send()?.text()?;
        let challenge_data: Session = serde_xml_rs::from_str(&challenge_data_raw)?;
        let challenge = challenge_data.challenge;

        let mut buf: Vec<u8> = vec![];
        for char in (format!("{}-{}", challenge, self.config.password)).encode_utf16() {
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
            .finalize_fixed_reset()
            .to_vec()
            .iter()
            .map(|byte| format!("{:02x}", byte))
            .collect::<String>();

        let response_text = format!("{challenge}-{sum}");
        let auth_response = self
            .client
            .post(format!("{base}/login_sid.lua", base = self.config.base_url))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(form_data!(&[
                ("username", self.config.username),
                ("response", response_text.as_str()),
            ]))
            .send()?
            .text()?;

        let session: Session = serde_xml_rs::from_str(&auth_response)?;
        if session.is_default_sid() {
            return Err(LoginError::boxed());
        }

        self.session = session;
        Ok(&self.session)
    }

    fn overview(&self) -> AnyError<Overview> {
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

    fn devices(&self) -> AnyError<Devices> {
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
        let url = format!("{base}/data.lua", base = self.config.base_url);
        let res = self
            .client
            .post(url)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(form_data![&[
                ("sid", self.session.sid.as_str()),
                ("page", "reboot"),
                ("reboot", "0"),
            ]])
            .send()?;

        let t = res.bytes()?;

        let result: serde_json::Value = serde_json::from_slice(t.as_ref())?;
        let status = (&result.pointer("/data/reboot")).and_then(serde_json::Value::as_str);

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

    fn disconnect(&self) -> rw::Result<serde_json::Value> {
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

    fn connect(&self) -> rw::Result<serde_json::Value> {
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
    connection: String,
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
    fn info(&self, api: &FritzClient, _args: &Args) -> AnyError<()> {
        let overview = api.overview()?;

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

    fn reboot(&self, api: &FritzClient, _args: &Args) -> AnyError<()> {
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

    fn reconnect(&self, api: &FritzClient, _args: &Args) -> rw::Result<()> {
        api.disconnect()?;
        api.connect()?;
        println!("Heads up! This can take up to 30s to take full effect..");
        Ok(())
    }

    fn devices(&self, api: &FritzClient, _args: &Args) -> AnyError<()> {
        let data = api.devices()?;
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
                    row.connection = typ;
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
            // TODO: save sid with expire date to session.json and reuse?
            let base_url = env::var_os("FRITZ_URL")
                .expect("expected FRITZ_URL env var")
                .into_string()
                .expect("expected FRITZ_URL env var to be valid");
            let username = env::var_os("FRITZ_USER")
                .expect("expected FRITZ_USER env var")
                .into_string()
                .expect("expected FRITZ_USER env var to be valid");
            let password = env::var_os("FRITZ_PASSWORD")
                .expect("expected FRITZ_PASSWORD env var")
                .into_string()
                .expect("expected FRITZ_PASSWORD env var to be valid");
            let config = Config {
                base_url: base_url.as_str(),
                username: username.as_str(),
                password: password.as_str(),
            };
            let mut api = FritzClient::new_with_config(config);
            api.login()?;
            match command {
                Commands::Info(args) => self.info(&api, args)?,
                Commands::Reboot(args) => self.reboot(&api, args)?,
                Commands::Reconnect(args) => self.reconnect(&api, args)?,
                Commands::Devices(args) => self.devices(&api, args)?,
            }
        }
        Ok(())
    }
}
