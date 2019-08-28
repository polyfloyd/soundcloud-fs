use crate::soundcloud;
use chrono::Datelike;
use id3;
use log::*;
use std::io;

pub fn tag_for_track(
    track: &soundcloud::Track,
    enable_artwork: bool,
    parse_strings: bool,
) -> Result<impl io::Read + io::Seek, soundcloud::Error> {
    let mut tag = id3::Tag::new();

    if let Some(i) = track.title.find(" - ").filter(|_| parse_strings) {
        tag.set_title(&track.title[..i]);
        tag.set_artist(&track.title[i + 3..]);
    } else {
        tag.set_artist(track.user.username.as_str());
        tag.set_title(track.title.as_str());
    }

    tag.set_duration(track.duration_ms as u32);
    tag.set_text("TCOP", track.license.as_str());
    tag.add_frame(id3::Frame::with_content(
        "WOAF",
        id3::Content::Link(track.permalink_url.to_string()),
    ));
    tag.add_frame(id3::Frame::with_content(
        "WOAR",
        id3::Content::Link(track.user.permalink_url.to_string()),
    ));
    tag.set_year(
        track
            .release_year
            .unwrap_or_else(|| track.created_at.date().year()),
    );
    tag.set_text(
        "TDAT",
        format!(
            "{:02}{:02}",
            track.created_at.date().day(),
            track.created_at.date().month(),
        ),
    );
    if let Some(ref descrtiption) = track.description {
        tag.add_comment(id3::frame::Comment {
            lang: "eng".to_string(),
            description: "Description".to_string(),
            text: descrtiption.clone(),
        });
    }
    if let Some(year) = track.release_year {
        tag.set_text("TORY", format!("{}", year));
    }
    if let Some(ref genre) = track.genre {
        tag.set_genre(genre.as_str());
    }
    if let Some(bpm) = track.bpm {
        tag.set_text("TBPM", format!("{}", bpm.round()));
    }
    if let Some(ref label) = track.label_name {
        tag.set_text("TPUB", label.as_str());
    }
    if let Some(ref isrc) = track.isrc {
        tag.set_text("TSRC", isrc.as_str());
    }

    if enable_artwork {
        match track.artwork() {
            Err(soundcloud::Error::ArtworkNotAvailable) => (),
            Err(err) => error!("{}", err),
            Ok((data, mime_type)) => tag.add_picture(id3::frame::Picture {
                mime_type,
                picture_type: id3::frame::PictureType::CoverFront,
                description: "Artwork".to_string(),
                data,
            }),
        }
    }

    let mut id3_tag_buf = Vec::new();
    tag.write_to(&mut id3_tag_buf, id3::Version::Id3v24)
        .unwrap();
    Ok(io::Cursor::new(id3_tag_buf))
}
