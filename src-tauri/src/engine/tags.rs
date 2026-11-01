//! `engine/tags.rs` — audio tag + cover-art carry-over, in-process (lofty).
//!
//! ffmpeg is told `-map_metadata -1` (`ffmpeg.rs`), because its tag mapping is
//! lossy: measured on a tagged MP3 with an attached cover, `-map_metadata 0`
//! reproduced title/artist/album/date but re-mapped the cover stream's comment
//! and re-stamped `encoder=Lavf…`. One source of truth is the source file, read
//! by lofty, which understands ID3v2, Vorbis comments, MP4 ilst and FLAC
//! pictures natively.
//!
//! Failure policy: a tag merge that cannot be applied is **reported**, never
//! silently dropped and never fatal to the conversion (§6.0.4 — no silent
//! failures). The audio bytes are already correct at that point.

use std::path::Path;

use lofty::config::WriteOptions;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::tag::{Tag, TagType};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeReport {
    /// Number of textual items copied.
    pub items: usize,
    /// Number of pictures copied.
    pub pictures: usize,
    /// Tag family written (`Id3v2`, `VorbisComments`, `Mp4Ilst`, `RiffInfo`, …).
    pub tag_type: Option<TagType>,
    /// True when the source had no tags to begin with — not an error.
    pub source_untagged: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TagError {
    SourceUnreadable(String),
    TargetUnreadable(String),
    Write(String),
}

impl std::fmt::Display for TagError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TagError::SourceUnreadable(e) => write!(f, "cannot read source tags: {e}"),
            TagError::TargetUnreadable(e) => write!(f, "cannot read target tags: {e}"),
            TagError::Write(e) => write!(f, "cannot write tags: {e}"),
        }
    }
}

/// Which tag family the target container can actually hold, chosen from the
/// **file name we are writing**.
///
/// Not from what the file currently has: ffmpeg writes an Ogg/M4A with no tag
/// block at all, so "use the target's existing tag type" fell back to ID3v2 and
/// wrote an ID3v2 tag into an Ogg container — lofty could read it back, ffprobe
/// (and every player) saw nothing. Measured.
fn tag_type_for(target: &Path) -> Option<TagType> {
    let ext = target
        .extension()?
        .to_string_lossy()
        .to_ascii_lowercase();
    Some(match ext.as_str() {
        "mp3" => TagType::Id3v2,
        "flac" | "ogg" | "opus" => TagType::VorbisComments,
        "m4a" | "mp4" | "m4b" => TagType::Mp4Ilst,
        "wav" | "wave" => TagType::RiffInfo,
        _ => return None,
    })
}

/// Which tag family the target file can hold, in preference order.
fn target_tag_type(file: &lofty::file::TaggedFile) -> Option<TagType> {
    if let Some(tag) = file.primary_tag() {
        return Some(tag.tag_type());
    }
    file.first_tag().map(|t| t.tag_type())
}

/// Copy every tag item and picture from `from` onto `to`.
pub fn merge(from: &Path, to: &Path) -> Result<MergeReport, TagError> {
    let source = lofty::read_from_path(from)
        .map_err(|e| TagError::SourceUnreadable(e.to_string()))?;
    let source_tag = match source.primary_tag().or_else(|| source.first_tag()) {
        Some(tag) => tag,
        None => {
            return Ok(MergeReport {
                items: 0,
                pictures: 0,
                tag_type: None,
                source_untagged: true,
            })
        }
    };

    let mut target = lofty::read_from_path(to)
        .map_err(|e| TagError::TargetUnreadable(e.to_string()))?;
    let tag_type = tag_type_for(to)
        .or_else(|| target_tag_type(&target))
        .unwrap_or(TagType::Id3v2);

    let mut tag = Tag::new(tag_type);
    let mut items = 0usize;
    for item in source_tag.items() {
        tag.push(item.clone());
        items += 1;
    }
    let mut pictures = 0usize;
    for picture in source_tag.pictures() {
        tag.push_picture(picture.clone());
        pictures += 1;
    }

    target.insert_tag(tag);
    target
        .save_to_path(to, WriteOptions::default())
        .map_err(|e| TagError::Write(e.to_string()))?;

    Ok(MergeReport {
        items,
        pictures,
        tag_type: Some(tag_type),
        source_untagged: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_is_an_error_not_a_panic() {
        let p = std::env::temp_dir().join("ezc-no-such-audio.mp3");
        assert!(matches!(
            merge(&p, &p),
            Err(TagError::SourceUnreadable(_))
        ));
    }

    #[test]
    fn error_messages_name_the_failure() {
        let e = TagError::Write("disk full".into());
        assert!(e.to_string().contains("disk full"));
    }
}
