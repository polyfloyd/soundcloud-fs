extern crate env_logger;
extern crate failure;
#[macro_use]
extern crate failure_derive;
extern crate fuse;
extern crate libc;
#[macro_use]
extern crate log;
extern crate soundcloud;
extern crate time;

mod filesystem;
mod mapping;

use self::filesystem::*;
use self::mapping::*;
use std::env;
use std::path::Path;
use std::process;

fn main() {
    env_logger::init();

    let client_cache_path = "/tmp/sc-test-token";
    let username = env::var("SC_USERNAME").unwrap();
    let password = env::var("SC_PASSWORD").unwrap();

    let sc_client_rs = soundcloud::Client::from_cache(client_cache_path)
        .map(|v| {
            info!("loaded client from cache");
            v
        }).or_else(|err| {
            info!("{}", err);
            soundcloud::Client::login(username, password)
        });
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

    let myself = soundcloud::User::me(&sc_client).unwrap();

    let fs = FS::new(NodeCache::new(Entry::User(myself)));
    let path = Path::new("/home/polyfloyd/sc-test");
    fuse::mount(fs, &path, &[]).unwrap();
}
