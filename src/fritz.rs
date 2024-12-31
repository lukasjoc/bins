use dotenv::dotenv;
use form_urlencoded;
use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};
use std::env;

#[derive(clap::Parser)]
struct Args {}

#[derive(clap::Subcommand)]
enum Commands {
    /// Display debugging information about the FritzBox.
    Info(Args),
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

#[derive(Debug)]
struct FritzClient<'a> {
    base_url: &'a str,
    session_info: SessionInfo,
}

impl<'a> FritzClient<'a> {
    fn authenticate(base_url: &'a str, username: &str, password: &str) -> reqwest::Result<Self> {
        let client = reqwest::blocking::Client::new();

        // getting the challenge data
        let challenge_response = client
            .get(format!("{base_url}/login_sid.lua"))
            .send()?
            .text()?;
        let info: SessionInfo =
            serde_xml_rs::from_str(&challenge_response).expect("Should get the session info");
        //

        // Prepare the response data payload for the login request
        let mut buf: Vec<u8> = vec![];
        for char in (format!("{}-{}", info.challenge, password)).encode_utf16() {
            let ch = match char {
                ch if ch > 255 => 0x2e,
                other => other,
            };
            use std::io::Write;
            Write::write(&mut buf, &ch.to_le_bytes()).expect("write should work");
        }

        let mut hasher = Md5::new();
        hasher.update(&buf);
        let sum: String = hasher
            .finalize()
            .to_vec()
            .iter()
            .map(|x| format!("{:02x}", x))
            .collect();

        let body_payload: String = form_urlencoded::Serializer::new(String::new())
            .append_pair("username", username)
            .append_pair("response", &format!("{}-{}", info.challenge, sum))
            .finish();

        let auth_response = client
            .post(format!("{base_url}/login_sid.lua"))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body_payload)
            .send()?
            .text()?;

        let session_info: SessionInfo =
            serde_xml_rs::from_str(&auth_response).expect("Should get the session info");

        // TODO: reject with error if sids of challenge response and auth_response match.
        Ok(Self {
            base_url,
            session_info,
        })
    }
}

#[derive(clap::Parser)]
pub(crate) struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

impl Cli {
    fn info(&self, _args: &Args) -> reqwest::Result<()> {
        todo!()
    }
    pub(crate) fn run(&self) -> reqwest::Result<()> {
        dotenv().ok();
        let base_url = env::var("BASE_URL").expect("Need env var.");
        let username = env::var("USERNAME").expect("Need env var.");
        let password = env::var("PASSWORD").expect("Need env var.");

        let client = FritzClient::authenticate(&base_url, &username, &password)?;
        println!("BASE_URL: {:?}", client.base_url);
        println!("INFO: {:?}", client.session_info);

        if let Some(command) = &self.command {
            match command {
                Commands::Info(args) => self.info(args)?,
            }
        }
        Ok(())
    }
}
