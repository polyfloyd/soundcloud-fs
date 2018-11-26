extern crate chrono;
extern crate env_logger;
extern crate failure;
#[macro_use]
extern crate failure_derive;
extern crate fuse;
extern crate id3;
extern crate libc;
#[macro_use]
extern crate log;
extern crate soundcloud;
extern crate time;

mod filesystem;
mod ioutil;
mod mapping;

use self::filesystem::*;
use self::mapping::*;
use std::env;
use std::path::Path;
use std::process;

fn main() {
    env_logger::init();

    let username = env::var("SC_USERNAME").unwrap();
    let password = env::var("SC_PASSWORD").ok();
    let client_cache_path = "/tmp/sc-test-token";

    let sc_client_rs = match password {
        None => {
            info!("creating anonymous client");
            soundcloud::Client::anonymous()
        }
        Some(pw) => soundcloud::Client::from_cache(client_cache_path)
            .map(|v| {
                info!("loaded client from cache");
                v
            }).or_else(|err| {
                error!("{}", err);
                info!("logging in as {}", username);
                soundcloud::Client::login(&username, pw)
            }),
    };

    let sc_client = match sc_client_rs {
        Ok(v) => v,
        Err(err) => {
            error!("could not initialize SoundCloud client: {}", err);
            process::exit(1);
        }
    };
    if let Err(err) = sc_client.cache_to(client_cache_path) {
        error!(
            "could not cache SoundCloud client to {}: {}",
            client_cache_path, err
        );
    }

    let user = soundcloud::User::by_name(&sc_client, &username).unwrap();
    let fs = FS::new(NodeCache::new(Entry::User {
        user,
        recurse: true,
    }));
    let path = Path::new("/home/polyfloyd/sc-test");
    fuse::mount(fs, &path, &[]).unwrap();
}
