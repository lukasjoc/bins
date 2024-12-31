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
    /// List known devices on the network.
    Devices(Args),
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Default)]
struct SessionInfo {
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
    session_info: SessionInfo,
    client: reqwest::blocking::Client,
}

impl FritzApi {
    fn new_authenticate(config: Config) -> reqwest::Result<Self> {
        let client = reqwest::blocking::Client::new();

        // getting the challenge data
        let challenge_response = client
            .get(format!("{base}/login_sid.lua", base = config.base_url))
            .send()?
            .text()?;
        let info: SessionInfo =
            serde_xml_rs::from_str(&challenge_response).expect("Should get the session info");

        // Prepare the response data payload for the login request
        let mut buf: Vec<u8> = vec![];
        for char in (format!("{}-{}", info.challenge, config.password)).encode_utf16() {
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
            .append_pair("username", &config.username)
            .append_pair("response", &format!("{}-{}", info.challenge, sum))
            .finish();

        let auth_response = client
            .post(format!("{base}/login_sid.lua", base = config.base_url))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(payload)
            .send()?
            .text()?;

        let session_info: SessionInfo =
            serde_xml_rs::from_str(&auth_response).expect("Should get the session info");

        // TODO: reject with error if sids of challenge response and auth_response match.
        Ok(Self {
            config,
            session_info,
            client,
        })
    }
}

#[derive(clap::Parser)]
pub(crate) struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}
impl Cli {
    fn info(&self, api: &FritzApi, _args: &Args) -> reqwest::Result<()> {
        let sid = &api.session_info.sid;
        println!("SID: {sid}");
        Ok(())
    }
    pub(crate) fn run(&self) -> reqwest::Result<()> {
        let file = fs::File::open("./config.json").unwrap();
        let config: Config = serde_json::from_reader(file).unwrap();
        let api = FritzApi::new_authenticate(config)?;
        if let Some(command) = &self.command {
            match command {
                Commands::Info(args) => self.info(&api, args)?,
                _ => unimplemented!("Command not implemented yet!"),
            }
        }
        Ok(())
    }
}
