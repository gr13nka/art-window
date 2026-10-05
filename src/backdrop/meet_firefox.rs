//! Google Meet in Firefox: the background is two blobs in a browser database, and
//! can only be replaced by something of exactly the same length.
//!
//! Five things were established by hand on Firefox 156, macOS 12.7, and the design
//! stands on them:
//!
//! 1. Meet keeps an uploaded custom background in IndexedDB (`meet_fx_db`), and
//!    Firefox stores its blobs as plain files:
//!    `~/Library/Application Support/Firefox/Profiles/<profile>/storage/default/
//!    https+++meet.google.com/idb/191533160mbede_tx_f.files/<n>`, where `<n>` is a
//!    small integer. One upload makes two files written at the same moment: the
//!    image (lossless WebP, as Meet writes it; 234,198 bytes in the test) and a
//!    thumbnail (10,782 bytes). The directory also holds a `journals`
//!    subdirectory. With Firefox containers the origin directory name carries a
//!    suffix (`https+++meet.google.com^userContextId=…`), so it is matched by its
//!    start.
//! 2. The byte length of each blob is recorded inside the database, which is not
//!    ours to edit while Firefox runs. **A replacement must be exactly as many
//!    bytes as the file it replaces.**
//! 3. Firefox decodes by content, not by the recorded `image/webp` type: a JPEG in
//!    that file is shown. This was verified with a JPEG padded to the exact length
//!    by `COM` segments (`FF FE`, a two-byte big-endian length that counts itself,
//!    then filler) inserted **directly after the SOI marker**. That is the
//!    placement that was verified, so it is the one used.
//! 4. Replacing the file while Firefox is running works; no restart.
//! 5. Meet reads the file once per call, a few seconds after the camera comes on,
//!    and never again in that call — not when the camera is turned off and on, and
//!    a file swapped mid-call does not change the live background. The next call
//!    shows the new file.
//!
//! The file's access time records that read exactly as it does for zoom.us (APFS
//! moves it only when it is not later than the modification time), but here the
//! access time alone is not proof of a meeting: something else was once seen
//! reading the file with no call in progress. So a call is *live* when a camera is
//! in use, by anything, and a painting is shown when it has been read and the
//! camera is on. It is replaced when the camera is off again.

use super::{arm, move_into_place, probe, read_since_armed, Stage, Stale};
use crate::journal;
use crate::placement;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// The names of the two rendered files inside the spare directory.
const SPARE_IMAGE: &str = "image";
const SPARE_THUMBNAIL: &str = "thumbnail";

/// The origin directory's name, before any container suffix.
const ORIGIN: &str = "https+++meet.google.com";
/// The directory of `meet_fx_db`'s blobs, inside the origin's `idb`.
const BLOBS: &str = "191533160mbede_tx_f.files";
/// Below this the uploaded image cannot hold a painting at a decent quality.
const MIN_IMAGE: u64 = 100_000;
/// The sizes the image is tried at, largest first.
const IMAGE_SIZES: [(u32, u32); 3] = [(1920, 1080), (1280, 720), (960, 540)];
/// The thumbnail's size; it is only ever shown small.
const THUMBNAIL_SIZE: (u32, u32) = (160, 90);
/// The least a `COM` segment can be: marker, length, no filler.
const MIN_COM: usize = 4;
/// The most: two marker bytes, then a length field of 65,535 that counts itself,
/// which leaves 65,533 bytes of filler.
const MAX_COM: usize = 2 + 65_535;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeetFirefox {
    image: PathBuf,
    thumbnail: PathBuf,
}

impl MeetFirefox {
    pub fn adopt() -> Result<MeetFirefox> {
        match profiles_folder() {
            Some(root) => Self::adopt_in(&root),
            None => bail!("Google Meet in Firefox is not supported on this system"),
        }
    }

    /// Looks through every profile under `profiles` for Meet's blob directory. An
    /// upload writes the image and its thumbnail at once, so the two newest files
    /// in it are that pair, the larger being the image. Of several directories —
    /// profiles, containers — the one whose image is newest wins: it is the one the
    /// user just uploaded to.
    fn adopt_in(profiles: &Path) -> Result<MeetFirefox> {
        let newest = fs::read_dir(profiles)
            .ok()
            .into_iter()
            .flatten()
            .flatten()
            .flat_map(|profile| origins(&profile.path().join("storage/default")))
            .filter_map(|origin| upload_in(&origin.join("idb").join(BLOBS)))
            .max_by_key(|upload| upload.image_modified);
        let Some(upload) = newest else {
            bail!("Upload a picture as your background in Google Meet (Firefox) first");
        };
        if upload.image_len < MIN_IMAGE {
            bail!(
                "The picture uploaded to Meet is too small to hold a painting; upload a larger one"
            );
        }
        Ok(MeetFirefox {
            image: upload.image,
            thumbnail: upload.thumbnail,
        })
    }

    #[cfg(test)]
    pub fn at(image: PathBuf, thumbnail: PathBuf) -> MeetFirefox {
        MeetFirefox { image, thumbnail }
    }
}

impl Stage for MeetFirefox {
    /// Two files, each already the length of the one it will replace, so that
    /// hanging is two renames and nothing in the database has to change.
    fn render(&self, painting: &Path, spare: &Path) -> Result<()> {
        let image_len = length(&self.image)?;
        let thumbnail_len = length(&self.thumbnail)?;
        // A COM segment is at least four bytes, and padding cannot be taken back.
        let image =
            placement::cover_to_fit(painting, &IMAGE_SIZES, image_len.saturating_sub(MIN_COM))?;
        let thumbnail = placement::cover_to_fit(
            painting,
            &[THUMBNAIL_SIZE],
            thumbnail_len.saturating_sub(MIN_COM),
        )?;
        for (name, jpeg, len) in [
            (SPARE_IMAGE, image, image_len),
            (SPARE_THUMBNAIL, thumbnail, thumbnail_len),
        ] {
            let path = spare.join(name);
            journal::note!(
                "backdrop",
                "MeetFirefox: {name} fitted to {} bytes, padded to {len}",
                jpeg.len()
            );
            fs::write(&path, pad_to(jpeg, len)?)
                .with_context(|| format!("writing {}", path.display()))?;
        }
        Ok(())
    }

    /// Both files must still be there and still the lengths the spare was made for;
    /// otherwise the user has changed their background in Meet, and the spare is
    /// useless. Nothing is moved until both agree, so a half-swapped pair cannot
    /// happen. The thumbnail needs no arming: only the image is watched.
    fn hang(&self, spare: &Path) -> Result<()> {
        for (slot, name) in [
            (&self.image, SPARE_IMAGE),
            (&self.thumbnail, SPARE_THUMBNAIL),
        ] {
            let made_for = length(&spare.join(name))?;
            match fs::metadata(slot) {
                Ok(meta) if meta.is_file() && meta.len() as usize == made_for => {}
                _ => {
                    return Err(Stale(format!(
                        "{} is not the file this painting was made for; the background was changed in Google Meet",
                        slot.display()
                    ))
                    .into())
                }
            }
        }
        move_into_place(&spare.join(SPARE_IMAGE), &self.image)?;
        move_into_place(&spare.join(SPARE_THUMBNAIL), &self.thumbnail)?;
        arm(&self.image)
    }

    fn read_since_hung(&self) -> bool {
        read_since_armed(&self.image)
    }

    fn live(&self) -> bool {
        probe::camera_in_use()
    }
}

fn length(path: &Path) -> Result<usize> {
    let meta = fs::metadata(path).with_context(|| format!("examining {}", path.display()))?;
    Ok(meta.len() as usize)
}

/// Makes `jpeg` exactly `target` bytes by inserting `COM` segments straight after
/// the SOI marker, which Firefox is known to skip over.
///
/// One segment holds at most 65,533 bytes of filler, so a big gap takes several.
/// The gap is split so that no remainder is left that is too small to be a segment
/// of its own: a one-to-three byte remainder would have nowhere to go.
fn pad_to(jpeg: Vec<u8>, target: usize) -> Result<Vec<u8>> {
    const SOI: [u8; 2] = [0xFF, 0xD8];
    if !jpeg.starts_with(&SOI) {
        bail!("not a JPEG: it does not begin with an SOI marker");
    }
    let Some(mut gap) = target.checked_sub(jpeg.len()) else {
        bail!(
            "the JPEG is {} bytes, over the {target} it must fit",
            jpeg.len()
        );
    };
    if gap != 0 && gap < MIN_COM {
        bail!("a gap of {gap} bytes is too small for a COM segment");
    }
    let mut out = Vec::with_capacity(target);
    out.extend_from_slice(&SOI);
    while gap > 0 {
        let mut segment = gap.min(MAX_COM);
        if gap > segment && gap - segment < MIN_COM {
            segment = gap - MIN_COM;
        }
        out.extend_from_slice(&[0xFF, 0xFE]);
        // The length field counts itself but not the marker.
        out.extend_from_slice(&((segment - 2) as u16).to_be_bytes());
        out.resize(out.len() + segment - MIN_COM, b' ');
        gap -= segment;
    }
    out.extend_from_slice(&jpeg[SOI.len()..]);
    Ok(out)
}

/// The image and thumbnail an upload wrote, as they are on disk now.
struct Upload {
    image: PathBuf,
    thumbnail: PathBuf,
    image_len: u64,
    image_modified: SystemTime,
}

/// The two newest regular files in `blobs`, or nothing when there are fewer than
/// two. Subdirectories, `journals` among them, are not files and never count.
fn upload_in(blobs: &Path) -> Option<Upload> {
    let mut files: Vec<(SystemTime, u64, PathBuf)> = fs::read_dir(blobs)
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let meta = entry.metadata().ok()?;
            meta.is_file()
                .then_some((meta.modified().ok()?, meta.len(), entry.path()))
        })
        .collect();
    files.sort_by_key(|file| std::cmp::Reverse(file.0));
    let mut pair: Vec<_> = files.into_iter().take(2).collect();
    if pair.len() < 2 {
        return None;
    }
    pair.sort_by_key(|(_, len, _)| std::cmp::Reverse(*len));
    let (image_modified, image_len, image) = pair.remove(0);
    let (_, _, thumbnail) = pair.remove(0);
    Some(Upload {
        image,
        thumbnail,
        image_len,
        image_modified,
    })
}

/// Meet's origin directories under one profile's `storage/default`: the plain one,
/// and one per Firefox container, which carries a `^…` suffix.
fn origins(default_storage: &Path) -> Vec<PathBuf> {
    fs::read_dir(default_storage)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| {
            let name = entry.file_name();
            name.to_str()
                .and_then(|n| n.strip_prefix(ORIGIN))
                .is_some_and(|rest| rest.is_empty() || rest.starts_with('^'))
        })
        .map(|entry| entry.path())
        .collect()
}

#[cfg(target_os = "macos")]
fn profiles_folder() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join("Library/Application Support/Firefox/Profiles"))
}

#[cfg(not(target_os = "macos"))]
fn profiles_folder() -> Option<PathBuf> {
    None
}

#[cfg(test)]
mod tests {
    use super::super::{scratch, write_at};
    use super::*;
    use image::{Rgb, RgbImage};
    use std::time::Duration;

    /// Writes an upload into `profile`'s Meet directory: an image of `image_len`
    /// bytes and a thumbnail of 100, both `age` seconds old, and an older stray.
    fn upload(root: &Path, profile: &str, origin: &str, image_len: usize, age: u64) -> PathBuf {
        let blobs = root
            .join(profile)
            .join("storage/default")
            .join(origin)
            .join("idb")
            .join(BLOBS);
        fs::create_dir_all(blobs.join("journals")).unwrap();
        let at = SystemTime::now() - Duration::from_secs(age);
        write_at(
            &blobs.join("1"),
            &vec![0; 500],
            at - Duration::from_secs(5000),
        );
        write_at(&blobs.join("2"), &vec![0; image_len], at);
        write_at(&blobs.join("3"), &[0; 100], at);
        write_at(&blobs.join("journals/4"), &vec![0; 900_000], at);
        blobs
    }

    #[test]
    fn adopting_takes_the_newest_pair_and_the_larger_is_the_image() {
        let root = scratch("meet-adopt");
        let blobs = upload(&root, "a.default", ORIGIN, 234_198, 10);
        let got = MeetFirefox::adopt_in(&root).unwrap();
        assert_eq!(got.image, blobs.join("2"));
        assert_eq!(got.thumbnail, blobs.join("3"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn adopting_finds_a_container_origin_and_prefers_the_newer_profile() {
        let root = scratch("meet-profiles");
        upload(&root, "old.default", ORIGIN, 200_000, 3000);
        let newer = upload(
            &root,
            "new.default",
            "https+++meet.google.com^userContextId=4",
            300_000,
            10,
        );
        upload(
            &root,
            "other.default",
            "https+++meet.google.com.evil",
            900_000,
            1,
        );
        let got = MeetFirefox::adopt_in(&root).unwrap();
        assert_eq!(got.image, newer.join("2"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn adopting_without_an_upload_or_with_a_tiny_one_says_what_to_do() {
        let root = scratch("meet-none");
        let message = MeetFirefox::adopt_in(&root).unwrap_err().to_string();
        assert!(message.starts_with("Upload a picture as your background"));
        upload(&root, "a.default", ORIGIN, 99_999, 10);
        let message = MeetFirefox::adopt_in(&root).unwrap_err().to_string();
        assert!(message.contains("too small"));
        let _ = fs::remove_dir_all(&root);
    }

    fn jpeg(width: u32, height: u32) -> Vec<u8> {
        let image = RgbImage::from_fn(width, height, |x, y| Rgb([x as u8, y as u8, 90]));
        let mut out = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 80)
            .encode_image(&image)
            .unwrap();
        out
    }

    #[test]
    fn padding_reaches_the_exact_length_and_still_decodes() {
        let source = jpeg(64, 48);
        // Including a few bytes over a segment boundary, and one to three short
        // of one, where a naive split leaves a remainder nothing can fill.
        let gaps = [
            4, 5, 7, 100, 65_537, 65_538, 65_540, 65_541, 65_544, 131_074, 200_001,
        ];
        for gap in gaps {
            let target = source.len() + gap;
            let padded = pad_to(source.clone(), target).unwrap();
            assert_eq!(padded.len(), target, "gap {gap}");
            assert_eq!(padded[..2], [0xFF, 0xD8]);
            let decoded = image::load_from_memory(&padded).unwrap();
            assert_eq!((decoded.width(), decoded.height()), (64, 48), "gap {gap}");
        }
        assert_eq!(pad_to(source.clone(), source.len()).unwrap(), source);
        assert!(pad_to(source.clone(), source.len() + 2).is_err());
        assert!(pad_to(source.clone(), source.len() - 1).is_err());
    }

    fn slot_files(dir: &Path, image: usize, thumbnail: usize) -> MeetFirefox {
        fs::write(dir.join("2"), vec![1; image]).unwrap();
        fs::write(dir.join("3"), vec![1; thumbnail]).unwrap();
        MeetFirefox::at(dir.join("2"), dir.join("3"))
    }

    #[test]
    fn hanging_replaces_both_files_when_the_lengths_agree() {
        let dir = scratch("meet-hang");
        let (slot, spare) = (dir.join("slot"), dir.join("spare"));
        fs::create_dir_all(&slot).unwrap();
        fs::create_dir_all(&spare).unwrap();
        let stage = slot_files(&slot, 100, 50);
        fs::write(spare.join(SPARE_IMAGE), vec![2; 100]).unwrap();
        fs::write(spare.join(SPARE_THUMBNAIL), vec![2; 50]).unwrap();
        stage.hang(&spare).unwrap();
        assert_eq!(fs::read(&stage.image).unwrap(), vec![2; 100]);
        assert_eq!(fs::read(&stage.thumbnail).unwrap(), vec![2; 50]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn hanging_refuses_when_a_slot_file_changed_length() {
        let dir = scratch("meet-stale");
        let (slot, spare) = (dir.join("slot"), dir.join("spare"));
        fs::create_dir_all(&slot).unwrap();
        fs::create_dir_all(&spare).unwrap();
        let stage = slot_files(&slot, 100, 51);
        fs::write(spare.join(SPARE_IMAGE), vec![2; 100]).unwrap();
        fs::write(spare.join(SPARE_THUMBNAIL), vec![2; 50]).unwrap();
        let error = stage.hang(&spare).unwrap_err();
        assert!(error.is::<Stale>());
        // Neither file moved, not even the one that still agreed.
        assert_eq!(fs::read(&stage.image).unwrap(), vec![1; 100]);
        assert!(spare.join(SPARE_IMAGE).exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rendering_makes_files_of_exactly_the_slot_lengths() {
        let dir = scratch("meet-render");
        let (slot, spare) = (dir.join("slot"), dir.join("spare"));
        fs::create_dir_all(&slot).unwrap();
        fs::create_dir_all(&spare).unwrap();
        let stage = slot_files(&slot, 120_000, 10_782);
        let painting = dir.join("painting.png");
        RgbImage::from_fn(800, 600, |x, y| Rgb([(x / 4) as u8, (y / 3) as u8, 120]))
            .save(&painting)
            .unwrap();
        stage.render(&painting, &spare).unwrap();
        assert_eq!(
            fs::metadata(spare.join(SPARE_IMAGE)).unwrap().len(),
            120_000
        );
        assert_eq!(
            fs::metadata(spare.join(SPARE_THUMBNAIL)).unwrap().len(),
            10_782
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
