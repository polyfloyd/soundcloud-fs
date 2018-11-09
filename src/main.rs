extern crate clap;
extern crate fuse;
extern crate time;
#[macro_use]
extern crate log;
extern crate env_logger;
extern crate failure;
#[macro_use]
extern crate failure_derive;
extern crate libc;
extern crate reqwest;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;

mod fs;
mod mapping;
mod soundcloud;

use self::fs::*;
use self::mapping::*;
use std::env;
use std::path::Path;
use std::process;

fn main() {
    env_logger::init();

    let username = env::var("SC_USERNAME").unwrap();
    let password = env::var("SC_PASSWORD").unwrap();
    let sc_client = match soundcloud::Client::login(username, password) {
        Ok(v) => v,
        Err(err) => {
            error!("could not initialize SoundCloud client: {}", err);
            process::exit(1);
        }
    };

    let fs = FS::new(Entry::User(soundcloud::User::new("polyfloyd")));
    let path = Path::new("/home/polyfloyd/sc-test");
    fuse::mount(fs, &path, &[]).unwrap();
}
