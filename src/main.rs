extern crate chrono;
extern crate clap;
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
use std::process;

fn main() {
    env_logger::init();

    let cli = clap::App::new("SoundCloud FS")
        .version("0.1.0")
        .author("polyfloyd <floyd@polyfloyd.net>")
        .about("A FUSE driver for SoundCloud audio")
        .arg(
            clap::Arg::with_name("path")
                .short("p")
                .long("path")
                .takes_value(true)
                .required(true)
                .help("Sets the target directory of the mount"),
        ).arg(
            clap::Arg::with_name("user")
                .short("u")
                .long("user")
                .takes_value(true)
                .required(true)
                .help("Sets the user to create directory and file entries for"),
        ).arg(
            clap::Arg::with_name("login")
                .long("login")
                .value_name("username:password")
                .takes_value(true)
                .validator(|s| match s.splitn(2, ":").count() {
                    2 => Ok(()),
                    c => Err(format!("bad credential format, split on : yields {} strings", c)),
                }).help("Logs in using a username and password instead of accessing the API anonymously"),
        ).get_matches();

    let login = cli.value_of("login").and_then(|s| {
        let mut i = s.splitn(2, ":");
        let u = i.next().unwrap();
        i.next().map(|p| (u, p))
    });

    let client_cache_path = "/tmp/sc-test-token";

    let sc_client_rs = match login {
        None => {
            info!("creating anonymous client");
            soundcloud::Client::anonymous()
        }
        Some((username, password)) => soundcloud::Client::from_cache(client_cache_path)
            .map(|v| {
                info!("loaded client from cache");
                v
            }).or_else(|err| {
                error!("{}", err);
                info!("logging in as {}", username);
                soundcloud::Client::login(&username, password)
            }),
    };

    let sc_client = match sc_client_rs {
        Ok(v) => v,
        Err(err) => {
            error!("could not initialize SoundCloud client: {}", err);
            process::exit(1);
        }
    };

    match sc_client.cache_to(client_cache_path) {
        Ok(_) => (),
        Err(soundcloud::Error::NoToken) => (),
        Err(err) => error!(
            "could not cache SoundCloud client to {}: {}",
            client_cache_path, err
        ),
    };

    let username = cli.value_of("user").unwrap();
    let user = soundcloud::User::by_name(&sc_client, &username).unwrap();
    let fs = FS::new(NodeCache::new(Entry::User {
        user,
        recurse: true,
    }));
    let path = cli.value_of("path").unwrap();
    fuse::mount(fs, &path, &[]).unwrap();
}
