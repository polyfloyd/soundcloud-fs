use crate::ioutil;
use lazy_static::lazy_static;
use std::io;

const FRAMES_FLAG: u32 = 0x0000_0001;
const BYTES_FLAG: u32 = 0x0000_0002;
//const TOC_FLAG: u32 = 0x0000_0004;
//const VBR_SCALE_FLAG: u32 = 0x0000_0008;

const MEAN_FRAME_SIZE: u64 = 417;

lazy_static! {
    pub static ref ZERO_FRAME: [u8; MEAN_FRAME_SIZE as usize] = {
        let mut buf = [0; MEAN_FRAME_SIZE as usize];
        buf[0x00..0x04].copy_from_slice(&[0xff, 0xfb, 0x90, 0x64]);
        buf
    };
}

pub fn zero_frames(count: u64) -> impl io::Read + io::Seek {
    ioutil::Pattern::new(&ZERO_FRAME[..], ZERO_FRAME.len() as u64 * count)
}

pub fn cbr_header(bytes: u64) -> Vec<u8> {
    let mut buf = vec![0; 417];

    // The MPEG header.
    buf[0x00..0x04].copy_from_slice(&[0xff, 0xfb, 0x90, 0x64]);

    // "Info" to indicate that this is a header for a CBR stream.
    buf[0x24..0x28].copy_from_slice(b"Info");

    // Header flags.
    let flags = FRAMES_FLAG | BYTES_FLAG;
    buf[0x28..0x2c].copy_from_slice(&flags.to_be_bytes());

    // 0x34..0x98: Table of contents used for seeking. Not relevant for CBR.

    // The number of frames in the file.
    if flags & FRAMES_FLAG != 0 {
        let frames = bytes / MEAN_FRAME_SIZE;
        assert!(frames <= u64::from(std::u32::MAX));
        buf[0x2c..0x30].copy_from_slice(&(frames as u32).to_be_bytes());
    }

    // The filesize in bytes.
    if flags & BYTES_FLAG != 0 {
        assert!(bytes <= u64::from(std::u32::MAX));
        buf[0x30..0x34].copy_from_slice(&(bytes as u32).to_be_bytes());
    }

    // 0x34..0x38: VBR scale, whatever that is.

    // There are also the enc_delay and enc_padding fields, we'll leave them 0.

    // The encoder version string. Usually, this is something like "LAME3.99".
    let encoder = concat!(env!("CARGO_PKG_NAME"), " v", env!("CARGO_PKG_VERSION"));
    copy_from_var_str(&mut buf[0x9c..0xb0], &encoder);

    buf
}

fn copy_from_var_str(buf: &mut [u8], s: &str) {
    let b = s.as_bytes();
    buf[..b.len()].copy_from_slice(b);
}

// 00000000: fffb 9064 0000 0000 0000 0000 0000 0000  ...d............
// 00000010: 0000 0000 0000 0000 0000 0000 0000 0000  ................
// 00000020: 0000 0000 496e 666f 0000 000f 0000 25bf  ....Info......%.
// 00000030: 003d a1f4 0003 0508 0b0d 1012 1517 1a1c  .=..............
// 00000040: 1f21 2426 292c 2e31 3336 383b 3d40 4245  .!$&),.1368;=@BE
// 00000050: 484a 4d4f 5254 5759 5c5e 6164 6669 6b6e  HJMORTWY\^adfikn
// 00000060: 7073 7578 7a7d 8082 8587 8a8c 8f91 9496  psuxz}..........
// 00000070: 999b 9ea1 a3a6 a8ab adb0 b2b5 b7ba bdbf  ................
// 00000080: c2c4 c7c9 ccce d1d3 d6d9 dbde e0e3 e5e8  ................
// 00000090: eaed eff2 f5f7 fafc 0000 0039 4c41 4d45  ...........9LAME
// 000000a0: 332e 3939 7201 aa00 0000 002e 4800 0014  3.99r.......H...
// 000000b0: 8024 049b 4e00 0080 003d a1f4 ed2b 38fc  .$..N....=...+8.
// 000000c0: 0000 0000 0000 0000 0000 0000 0000 0000  ................
// 000000d0: 0000 0000 0000 0000 0000 0000 0000 0000  ................
// 000000e0: 0000 0000 0000 0000 0000 0000 0000 0000  ................
// 000000f0: 0000 0000 0000 0000 0000 0000 0000 0000  ................
// 00000100: 0000 0000 0000 0000 0000 0000 0000 0000  ................
// 00000110: 0000 0000 0000 0000 0000 0000 0000 0000  ................
// 00000120: 0000 0000 0000 0000 0000 0000 0000 0000  ................
// 00000130: 0000 0000 0000 0000 0000 0000 0000 0000  ................
// 00000140: 0000 0000 0000 0000 0000 0000 0000 0000  ................
// 00000150: 0000 0000 0000 0000 0000 0000 0000 0000  ................
// 00000160: 0000 0000 0000 0000 0000 0000 0000 0000  ................
// 00000170: 0000 0000 0000 0000 0000 0000 0000 0000  ................
// 00000180: 0000 0000 0000 0000 0000 0000 0000 0000  ................
// 00000190: 0000 0000 0000 0000 0000 0000 0000 0000  ................
// 000001a0: 00                                       .
