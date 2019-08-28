SoundCloud FS
=============

This project implements a FUSE driver to serve audio files from SoundCloud. It
is optimized to avoid needless API requests and aid mass indexing by music
libraries, specifically MPD.

See also: https://polyfloyd.net/post/soundcloud-fuse-mpd/

## Usage
`soundcloud-fs --help` :)

## Notice
This program is intended to be used as an interoperability layer for other
software and not as a way to circumvent restrictions of the SoundCloud
platform. When playing around with this program, I ask that you:

* Do not mass-download content, consider the artists
* Do not make excessive requests to the platform, consider the SoundCloud engineers
