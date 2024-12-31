/*** UNDER CONSTRUCTION ***/
/*** WORK IN PROGGERS   ***/
// TODO: clean up unwraps/expects and fix up error handling
// TODO: save sid with expire date to session.json and reuse?
// TODO: xgd location for cache and config files.

use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};
use std::fs;

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

#[derive(Debug, Serialize, Deserialize, PartialEq, Default)]
struct Session {
    #[serde(rename = "SID")]
    sid: String,
    #[serde(rename = "Challenge")]
    challenge: String,
    #[serde(rename = "BlockTime")]
    block_time: i64,
}

#[derive(Debug, Serialize, Deserialize)]
struct Config {
    base_url: String,
    username: String,
    password: String,
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

    fn overview(&self) -> reqwest::Result<serde_json::Value> {
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

    fn devices(&self) -> reqwest::Result<serde_json::Value> {
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

        Ok(serde_json::from_str(&res).unwrap_or_default())
    }
}

#[derive(clap::Parser)]
pub(crate) struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

impl Cli {
    fn info(&self, api: &FritzApi, _args: &Args) -> reqwest::Result<()> {
        let data = api.overview()?;
        println!("{:#}", data);
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
        let data = api.devices()?;
        // TODO: tabular data lister using table crate
        println!("{:#}", data);
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
