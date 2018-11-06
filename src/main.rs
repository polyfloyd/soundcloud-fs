extern crate clap;
extern crate fuse;
extern crate time;
#[macro_use]
extern crate log;
extern crate env_logger;
extern crate libc;

mod fs;
mod mapping;
mod soundcloud;

use self::fs::*;
use self::mapping::*;
use std::path::Path;

fn main() {
    env_logger::init();

    let fs = FS::new(Entry::User(soundcloud::User::new("polyfloyd")));
    let path = Path::new("/home/polyfloyd/sc-test");
    fuse::mount(fs, &path, &[]).unwrap();
}
