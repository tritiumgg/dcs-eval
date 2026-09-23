//! The file a capture leaves: which of the names it may take is there,
//! whether it is finished, and how big the picture is.
//!
//! DCS writes a capture some frames after it was asked for and says nothing
//! when it is done, so the file is the only witness. It may be a `.png`, a
//! `.jpg` or a `.bmp`, because the format is the user's setting and no call
//! reports it, or it may carry the bare name and no extension at all, which
//! is what an abandoned capture leaves. [`find`] looks for all four.
//!
//! A file is finished when it ends the way its format ends: a PNG with its
//! `IEND` chunk, a JPEG with its end-of-image marker, a BMP when its header's
//! own size field matches the file. A size that has stopped growing would
//! only be a guess about when the encoder last got scheduled; the terminator
//! is the file saying so itself.
//!
//! A file with nothing in it is its own finding, [`Finding::Empty`], and not
//! merely one that is not finished yet. A capture is zero bytes for part of
//! its own writing, so an empty file is looked at again like any unfinished
//! one; but one that is still empty when the wait runs out is a capture DCS
//! abandoned, and the caller is told that rather than that nothing came.
//!
//! The width and height come from the header — for a JPEG from the frame
//! header, found by walking the segments one by one, because the metadata
//! can carry a thumbnail that is a whole JPEG of its own, frame header and
//! all, sitting in front of the picture's.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// A picture format DCS can be set to write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Png,
    Jpeg,
    Bmp,
}

impl Format {
    /// The extension DCS gives a file of this format, which is also how the
    /// format is named in an answer.
    #[must_use]
    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Bmp => "bmp",
        }
    }
}

/// The order a capture's names are tried in, and the order that breaks a
/// tie between two modified at the same instant. The bare name comes last.
const NAMED: [Option<Format>; 4] = [
    Some(Format::Png),
    Some(Format::Jpeg),
    Some(Format::Bmp),
    None,
];

/// A file under one of the names a capture may take.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub path: PathBuf,
    /// The format its extension names, or `None` for the bare name.
    pub named: Option<Format>,
    pub modified: SystemTime,
}

impl Candidate {
    /// What the file holds now. See [`examine`].
    pub fn examine(&self) -> io::Result<Finding> {
        examine(&self.path, self.named)
    }
}

/// The newest file in `dir` named `name` with `.png`, `.jpg`, `.bmp` or no
/// extension; where two were modified at the same instant, the one earlier
/// in that order. `None` when there is none, including when `dir` does not
/// exist: a directory DCS has never written a capture into is not created
/// here, because there is nothing in it to find.
///
/// The name is taken as already checked; it is joined to `dir` as given.
pub fn find(dir: &Path, name: &str) -> io::Result<Option<Candidate>> {
    let mut newest: Option<Candidate> = None;
    for named in NAMED {
        let path = match named {
            Some(format) => dir.join(format!("{name}.{}", format.extension())),
            None => dir.join(name),
        };
        let metadata = match fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        if !metadata.is_file() {
            continue;
        }
        let modified = metadata.modified()?;
        if newest
            .as_ref()
            .is_none_or(|found| modified > found.modified)
        {
            newest = Some(Candidate {
                path,
                named,
                modified,
            });
        }
    }
    Ok(newest)
}

/// A finished picture, as the answer describes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Picture {
    pub format: Format,
    pub bytes: u64,
    pub width: u32,
    pub height: u32,
}

/// What a capture's file holds when it is looked at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Finding {
    /// It ends the way its format ends, and its header gives the size.
    Whole(Picture),
    /// Not a whole picture yet: still being written, or not one at all.
    Unfinished,
    /// Zero bytes. Look again while the wait lasts; at its end, this is a
    /// capture DCS abandoned.
    Empty,
}

/// The Win32 error a file opened by another process without read sharing
/// gives, which is how a file that is still being written can look.
const ERROR_SHARING_VIOLATION: i32 = 32;

/// Read the file at `path` and judge it. The whole file is read each time:
/// the terminator is at the end and a JPEG's frame header is wherever the
/// segments in front of it put it, and a capture is a few megabytes at
/// most. A file the writer still holds shut against readers is
/// [`Finding::Unfinished`], because that is what it is.
pub fn examine(path: &Path, named: Option<Format>) -> io::Result<Finding> {
    match fs::read(path) {
        Ok(bytes) => Ok(judge(named, &bytes)),
        Err(error) if error.raw_os_error() == Some(ERROR_SHARING_VIOLATION) => {
            Ok(Finding::Unfinished)
        }
        Err(error) => Err(error),
    }
}

/// Judge a file's bytes. `named` is the format the extension names; a file
/// under the bare name is judged by what it starts with. A file whose
/// contents are not the format its name says is never whole.
#[must_use]
pub fn judge(named: Option<Format>, bytes: &[u8]) -> Finding {
    if bytes.is_empty() {
        return Finding::Empty;
    }
    let Some(format) = named.or_else(|| sniffed(bytes)) else {
        return Finding::Unfinished;
    };
    let size = match format {
        Format::Png => png(bytes),
        Format::Jpeg => jpeg(bytes),
        Format::Bmp => bmp(bytes),
    };
    match size {
        Some((width, height)) => Finding::Whole(Picture {
            format,
            bytes: bytes.len() as u64,
            width,
            height,
        }),
        None => Finding::Unfinished,
    }
}

const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

/// The `IEND` chunk: no data, so a zero length, the type, and its CRC,
/// which is the same in every PNG.
const PNG_END: [u8; 12] = [0, 0, 0, 0, b'I', b'E', b'N', b'D', 0xAE, 0x42, 0x60, 0x82];

const JPEG_START: [u8; 2] = [0xFF, 0xD8];
const JPEG_END: [u8; 2] = [0xFF, 0xD9];

/// The format a file's first bytes say it is.
fn sniffed(bytes: &[u8]) -> Option<Format> {
    if bytes.starts_with(&PNG_SIGNATURE) {
        Some(Format::Png)
    } else if bytes.starts_with(&JPEG_START) {
        Some(Format::Jpeg)
    } else if bytes.starts_with(b"BM") {
        Some(Format::Bmp)
    } else {
        None
    }
}

fn be16(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}

fn be32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

fn le16(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}

fn le32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

/// Both sides of a size, where neither is zero.
fn sized(width: u32, height: u32) -> Option<(u32, u32)> {
    (width > 0 && height > 0).then_some((width, height))
}

/// A whole PNG's size: the image header is always the first chunk.
fn png(bytes: &[u8]) -> Option<(u32, u32)> {
    let whole = bytes.ends_with(&PNG_END);
    if !whole || !bytes.starts_with(&PNG_SIGNATURE) || bytes.get(12..16)? != b"IHDR" {
        return None;
    }
    sized(be32(bytes, 16)?, be32(bytes, 20)?)
}

/// A whole BMP's size: from the information header, or from the older core
/// header's narrower fields. A negative height is a picture stored top row
/// first, and is as tall as a positive one.
fn bmp(bytes: &[u8]) -> Option<(u32, u32)> {
    let declared = usize::try_from(le32(bytes, 2)?).ok()?;
    if !bytes.starts_with(b"BM") || declared != bytes.len() {
        return None;
    }
    match le32(bytes, 14)? {
        12 => sized(u32::from(le16(bytes, 18)?), u32::from(le16(bytes, 20)?)),
        40.. => {
            let width = i32::from_le_bytes(bytes.get(18..22)?.try_into().ok()?);
            let height = i32::from_le_bytes(bytes.get(22..26)?.try_into().ok()?);
            sized(u32::try_from(width).ok()?, height.unsigned_abs())
        }
        _ => None,
    }
}

/// A whole JPEG's size, from its frame header.
fn jpeg(bytes: &[u8]) -> Option<(u32, u32)> {
    if !bytes.starts_with(&JPEG_START) || !bytes.ends_with(&JPEG_END) {
        return None;
    }
    let (width, height) = jpeg_frame(bytes)?;
    sized(width, height)
}

/// Whether a marker starts a frame header. `C4`, `C8` and `CC` sit in the
/// same range and are not frames.
fn is_frame(marker: u8) -> bool {
    matches!(marker, 0xC0..=0xCF) && !matches!(marker, 0xC4 | 0xC8 | 0xCC)
}

/// The width and height a frame header's body carries: after the sample
/// precision, the height and then the width.
fn frame_size(body: &[u8]) -> Option<(u32, u32)> {
    Some((u32::from(be16(body, 3)?), u32::from(be16(body, 1)?)))
}

/// Walk a JPEG's segments from the start of the image to the first frame
/// header, stepping over each by the length it declares. A thumbnail in the
/// metadata is inside one of those segments, so it is stepped over whole
/// and its frame header is never read. Reaching the scan or the end of the
/// image with no frame header, or a byte that is not a marker where one
/// should be, is no size at all.
fn jpeg_frame(bytes: &[u8]) -> Option<(u32, u32)> {
    let mut at = JPEG_START.len();
    loop {
        if *bytes.get(at)? != 0xFF {
            return None;
        }
        // Any number of fill bytes may stand in front of a marker.
        while *bytes.get(at)? == 0xFF {
            at += 1;
        }
        let marker = bytes[at];
        at += 1;
        match marker {
            0x01 | 0xD0..=0xD7 => continue,
            0xD8..=0xDA => return None,
            _ => {}
        }
        let length = usize::from(be16(bytes, at)?);
        let body = bytes.get(at + 2..at.checked_add(length)?)?;
        if is_frame(marker) {
            return frame_size(body);
        }
        at += length;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::Sandbox;
    use std::fs::File;
    use std::time::Duration;

    const PNG: &[u8] = include_bytes!("../fixtures/shot/picture.png");
    const JPEG: &[u8] = include_bytes!("../fixtures/shot/picture.jpg");
    const BMP: &[u8] = include_bytes!("../fixtures/shot/picture.bmp");
    const THUMBNAIL: &[u8] = include_bytes!("../fixtures/shot/thumbnail.jpg");

    /// The size every picture fixture was drawn at. The thumbnail is 8x6.
    const WIDTH: u32 = 37;
    const HEIGHT: u32 = 23;

    fn whole(format: Format, bytes: &[u8]) -> Finding {
        Finding::Whole(Picture {
            format,
            bytes: bytes.len() as u64,
            width: WIDTH,
            height: HEIGHT,
        })
    }

    /// Every cut of `bytes` short of its end, from one byte up, judged as a
    /// file of `format`; the lengths that were called whole.
    fn cuts_called_whole(format: Format, bytes: &[u8]) -> Vec<usize> {
        (1..bytes.len())
            .filter(|&len| matches!(judge(Some(format), &bytes[..len]), Finding::Whole(_)))
            .collect()
    }

    /// The picture fixture with the thumbnail fixture carried in an `APP1`
    /// segment straight after the start of image, where a camera puts its
    /// metadata, so the thumbnail's frame header comes first in the file.
    fn with_thumbnail() -> Vec<u8> {
        let mut body = b"Exif\0\0".to_vec();
        body.extend_from_slice(THUMBNAIL);
        let length = u16::try_from(body.len() + 2).expect("a segment's length fits");
        let mut bytes = JPEG_START.to_vec();
        bytes.extend_from_slice(&[0xFF, 0xE1]);
        bytes.extend_from_slice(&length.to_be_bytes());
        bytes.extend_from_slice(&body);
        bytes.extend_from_slice(&JPEG[2..]);
        bytes
    }

    #[test]
    fn a_whole_png_is_read_with_its_size() {
        assert_eq!(judge(Some(Format::Png), PNG), whole(Format::Png, PNG));
    }

    #[test]
    fn a_whole_jpeg_is_read_with_its_size() {
        assert_eq!(judge(Some(Format::Jpeg), JPEG), whole(Format::Jpeg, JPEG));
    }

    #[test]
    fn a_whole_bmp_is_read_with_its_size() {
        assert_eq!(judge(Some(Format::Bmp), BMP), whole(Format::Bmp, BMP));
    }

    #[test]
    fn a_png_cut_short_of_its_end_is_not_whole() {
        assert_eq!(cuts_called_whole(Format::Png, PNG), Vec::<usize>::new());
    }

    #[test]
    fn a_jpeg_cut_short_of_its_end_is_not_whole() {
        assert_eq!(cuts_called_whole(Format::Jpeg, JPEG), Vec::<usize>::new());
    }

    #[test]
    fn a_bmp_cut_short_of_its_end_is_not_whole() {
        assert_eq!(cuts_called_whole(Format::Bmp, BMP), Vec::<usize>::new());
    }

    #[test]
    fn a_zero_byte_file_is_empty() {
        for named in NAMED {
            assert_eq!(judge(named, &[]), Finding::Empty, "{named:?}");
        }
    }

    #[test]
    fn a_zero_byte_file_on_disk_is_empty() {
        let dir = Sandbox::new();
        let path = dir.join("abandoned");
        File::create(&path).expect("the file is made");
        assert_eq!(examine(&path, None).expect("it is read"), Finding::Empty);
    }

    #[test]
    fn a_thumbnail_in_the_metadata_is_not_the_picture() {
        let bytes = with_thumbnail();
        assert_eq!(
            judge(Some(Format::Jpeg), &bytes),
            whole(Format::Jpeg, &bytes)
        );
    }

    #[test]
    fn a_jpeg_cut_just_past_its_thumbnail_is_not_whole() {
        // The thumbnail ends with an end-of-image marker of its own, so a
        // file cut there ends the way a JPEG does. It has no frame header
        // of its own yet, and that is what says it is not finished.
        let bytes = with_thumbnail();
        let cut = JPEG_START.len() + 4 + b"Exif\0\0".len() + THUMBNAIL.len();
        assert!(bytes[..cut].ends_with(&JPEG_END));
        assert_eq!(
            judge(Some(Format::Jpeg), &bytes[..cut]),
            Finding::Unfinished
        );
        assert_eq!(cuts_called_whole(Format::Jpeg, &bytes), Vec::<usize>::new());
    }

    #[test]
    fn a_bmp_stored_top_row_first_is_as_tall() {
        let mut bytes = BMP.to_vec();
        let height = i32::try_from(HEIGHT).expect("it fits");
        bytes[22..26].copy_from_slice(&(-height).to_le_bytes());
        assert_eq!(judge(Some(Format::Bmp), &bytes), whole(Format::Bmp, &bytes));
    }

    #[test]
    fn a_bare_name_is_judged_by_what_it_holds() {
        assert_eq!(judge(None, PNG), whole(Format::Png, PNG));
        assert_eq!(judge(None, JPEG), whole(Format::Jpeg, JPEG));
        assert_eq!(judge(None, BMP), whole(Format::Bmp, BMP));
        assert_eq!(judge(None, b"not a picture"), Finding::Unfinished);
    }

    #[test]
    fn a_file_that_is_not_its_extensions_format_is_not_whole() {
        assert_eq!(judge(Some(Format::Jpeg), PNG), Finding::Unfinished);
        assert_eq!(judge(Some(Format::Png), BMP), Finding::Unfinished);
        assert_eq!(judge(Some(Format::Bmp), JPEG), Finding::Unfinished);
    }

    #[test]
    fn a_file_on_disk_is_examined() {
        let dir = Sandbox::new();
        let path = dir.join("shot.png");
        fs::write(&path, PNG).expect("the file is written");
        let found = find(&dir.path, "shot")
            .expect("the directory is read")
            .expect("the file is found");
        assert_eq!(found.named, Some(Format::Png));
        assert_eq!(
            found.examine().expect("it is read"),
            whole(Format::Png, PNG)
        );
    }

    #[cfg(windows)]
    #[test]
    fn a_file_held_shut_by_its_writer_is_unfinished() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = Sandbox::new();
        let path = dir.join("shot.png");
        fs::write(&path, PNG).expect("the file is written");
        let _writer = fs::OpenOptions::new()
            .write(true)
            .share_mode(0)
            .open(&path)
            .expect("the file is held");
        assert_eq!(
            examine(&path, Some(Format::Png)).expect("a held file is no error"),
            Finding::Unfinished
        );
    }

    /// Write `bytes` at `path` and set its modified time to `modified`.
    fn written(path: &Path, bytes: &[u8], modified: SystemTime) {
        fs::write(path, bytes).expect("the file is written");
        File::options()
            .write(true)
            .open(path)
            .and_then(|file| file.set_modified(modified))
            .expect("the time is set");
    }

    #[test]
    fn nothing_under_the_name_is_nothing_found() {
        let dir = Sandbox::new();
        fs::write(dir.join("other.png"), PNG).expect("the file is written");
        assert_eq!(
            find(&dir.path, "shot").expect("the directory is read"),
            None
        );
    }

    #[test]
    fn a_directory_never_written_is_nothing_found() {
        let dir = Sandbox::new();
        let never = dir.join("ScreenShots");
        assert_eq!(find(&never, "shot").expect("no error"), None);
        assert!(!never.exists(), "the directory is not created");
    }

    #[test]
    fn each_name_a_capture_may_take_is_found() {
        let dir = Sandbox::new();
        for (file, named) in [
            ("a.png", Some(Format::Png)),
            ("b.jpg", Some(Format::Jpeg)),
            ("c.bmp", Some(Format::Bmp)),
            ("d", None),
        ] {
            fs::write(dir.join(file), b"").expect("the file is written");
            let stem = &file[..1];
            let found = find(&dir.path, stem).expect("the directory is read");
            let found = found.unwrap_or_else(|| panic!("{file} is found"));
            assert_eq!(found.named, named, "{file}");
            assert_eq!(found.path, dir.join(file));
        }
    }

    #[test]
    fn the_newest_of_several_wins() {
        let dir = Sandbox::new();
        let then = SystemTime::now() - Duration::from_secs(60);
        written(&dir.join("shot.png"), PNG, then);
        written(&dir.join("shot.bmp"), BMP, then + Duration::from_secs(2));
        written(&dir.join("shot.jpg"), JPEG, then + Duration::from_secs(1));
        let found = find(&dir.path, "shot").expect("read").expect("found");
        assert_eq!(found.named, Some(Format::Bmp));
        assert_eq!(found.modified, then + Duration::from_secs(2));
    }

    #[test]
    fn a_tie_goes_png_jpg_bmp_then_bare() {
        let dir = Sandbox::new();
        let then = SystemTime::now() - Duration::from_secs(60);
        written(&dir.join("shot"), b"", then);
        written(&dir.join("shot.bmp"), BMP, then);
        let found = find(&dir.path, "shot").expect("read").expect("found");
        assert_eq!(found.named, Some(Format::Bmp));
        written(&dir.join("shot.jpg"), JPEG, then);
        let found = find(&dir.path, "shot").expect("read").expect("found");
        assert_eq!(found.named, Some(Format::Jpeg));
        written(&dir.join("shot.png"), PNG, then);
        let found = find(&dir.path, "shot").expect("read").expect("found");
        assert_eq!(found.named, Some(Format::Png));
    }

    #[test]
    fn a_directory_under_the_name_is_not_a_capture() {
        let dir = Sandbox::new();
        fs::create_dir(dir.join("shot.png")).expect("the directory is made");
        assert_eq!(find(&dir.path, "shot").expect("read"), None);
    }
}
