//! Baselines for `--baseline`: an earlier shot used as the reference image, so a
//! change that finished before `sight` started is still seen.
//!
//! Every saved shot gets a sidecar `<name>.sight.json` (`Meta`) recording what it
//! shows: target, window-relative crop, pixel size before scaling and the saved
//! size. `Loaded::prepare` checks it against the new capture and refuses to
//! compare images that are not the same view.
//!
//! JPEG (the default shot format) is lossy, so the live frame is run through the
//! same resize and JPEG encode/decode as `shot` does before comparing: identical
//! pixels then give identical results, and no tolerance is needed.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, ImageFormat, ImageReader, RgbImage, RgbaImage};
use serde::{Deserialize, Serialize};

use crate::error::SightError;
use crate::win::Rect;

/// JPEG quality used when saving shots (and when emulating that for comparison).
pub const JPEG_QUALITY: u8 = 80;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Size {
    pub w: u32,
    pub h: u32,
}

/// What a shot captured.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum TargetId {
    Window { hwnd: u64 },
    Region { rect: Rect },
}

/// Sidecar content: enough to tell whether another capture is comparable.
#[derive(Debug, Serialize, Deserialize)]
pub struct Meta {
    pub v: u32,
    pub target: TargetId,
    /// Window-relative `--crop`, if any.
    pub crop: Option<Rect>,
    /// Pixel size after `--crop`, before `--scale`/`--max-edge`.
    pub source: Size,
    /// Size of the saved image.
    pub image: Size,
    pub format: String,
}

pub fn sidecar_path(image: &Path) -> PathBuf {
    image.with_extension("sight.json")
}

pub fn write_meta(image: &Path, meta: &Meta) -> std::io::Result<()> {
    std::fs::write(
        sidecar_path(image),
        serde_json::to_string(meta).expect("meta serializes"),
    )
}

/// Resize exactly as `shot` does (nearest when enlarging, Lanczos when shrinking).
pub fn resize(img: &RgbaImage, ow: u32, oh: u32) -> RgbaImage {
    let (sw, sh) = img.dimensions();
    if (ow, oh) == (sw, sh) {
        return img.clone();
    }
    let filter = if u64::from(ow) * u64::from(oh) > u64::from(sw) * u64::from(sh) {
        FilterType::Nearest
    } else {
        FilterType::Lanczos3
    };
    image::imageops::resize(img, ow, oh, filter)
}

/// Encode `rgb` as JPEG the way `shot` saves it and decode it back.
fn jpeg_roundtrip(rgb: RgbImage) -> RgbImage {
    let mut buf = Vec::new();
    DynamicImage::ImageRgb8(rgb)
        .write_with_encoder(JpegEncoder::new_with_quality(&mut buf, JPEG_QUALITY))
        .expect("encoding to memory cannot fail");
    image::load_from_memory_with_format(&buf, ImageFormat::Jpeg)
        .expect("decoding our own JPEG")
        .to_rgb8()
}

/// A matched baseline, plus (baseline size, current size) when those differ.
pub type Prepared = (Arc<Baseline>, Option<(Size, Size)>);

/// A baseline file read from disk, not yet matched to a capture.
pub struct Loaded {
    img: RgbImage,
    lossy: bool,
    meta: Option<Meta>,
    path: PathBuf,
}

fn bad(msg: String, hint: &str) -> SightError {
    SightError::InvalidArgsHint {
        msg,
        hint: hint.into(),
    }
}

impl Loaded {
    pub fn load(path: &Path, warnings: &mut Vec<String>) -> Result<Self, SightError> {
        let fail = |e: &dyn std::fmt::Display| {
            bad(
                format!("cannot read --baseline {}: {e}", path.display()),
                "pass the data.path of an earlier `sight shot`",
            )
        };
        let reader = ImageReader::open(path)
            .map_err(|e| fail(&e))?
            .with_guessed_format()
            .map_err(|e| fail(&e))?;
        let lossy = reader.format() == Some(ImageFormat::Jpeg);
        let img = reader.decode().map_err(|e| fail(&e))?.to_rgb8();
        let meta = match std::fs::read_to_string(sidecar_path(path)) {
            Ok(s) => match serde_json::from_str::<Meta>(&s) {
                Ok(m) => Some(m),
                Err(_) => {
                    warnings.push("baseline_metadata_unreadable".into());
                    None
                }
            },
            Err(_) => {
                warnings.push("baseline_without_metadata".into());
                None
            }
        };
        Ok(Self {
            img,
            lossy,
            meta,
            path: path.to_path_buf(),
        })
    }

    /// Check this baseline against the capture about to run (`source` is its
    /// pixel size after `--crop`, before scaling) and make it comparable.
    ///
    /// A different pixel size is not an error when the baseline has metadata (a
    /// click often resizes or expands the window): the baseline then compares as
    /// changed, and the second value reports (baseline size, current size).
    pub fn prepare(
        &self,
        target: TargetId,
        crop: Option<Rect>,
        source: (u32, u32),
    ) -> Result<Prepared, SightError> {
        let name = self.path.display();
        let (w, h) = self.img.dimensions();
        let live = Size {
            w: source.0,
            h: source.1,
        };
        let saved = Size { w, h };
        let source_size = if let Some(m) = &self.meta {
            if m.target != target {
                return Err(bad(
                    format!("--baseline {name} was taken from a different target"),
                    "take the baseline with the same --hwnd/--pid/--region",
                ));
            }
            if m.crop != crop {
                return Err(bad(
                    format!("--baseline {name} was taken with a different --crop"),
                    "take the baseline with the same --crop as this command",
                ));
            }
            if m.image != saved {
                return Err(bad(
                    format!("--baseline {name} no longer matches its metadata (edited?)"),
                    "take a fresh baseline with `sight shot`",
                ));
            }
            m.source
        } else {
            // No metadata: the file can only be compared 1:1.
            saved
        };
        let resized = source_size != live;
        if resized && self.meta.is_none() {
            return Err(bad(
                format!(
                    "--baseline {name} shows {}x{} px but this capture is {}x{} px (after --crop, before scaling); \
                     it has no .sight.json next to it, so it is assumed to be unscaled",
                    source_size.w, source_size.h, live.w, live.h
                ),
                "take the baseline with `sight shot`, which also writes the .sight.json this check needs",
            ));
        }
        let baseline = Baseline {
            rgb: self.img.clone(),
            // For a resized baseline this stays its own size, so every live
            // frame (of another size) compares as changed.
            source: source_size,
            lossy: self.lossy,
            size_changed: resized,
        };
        Ok((Arc::new(baseline), resized.then_some((source_size, live))))
    }
}

/// A baseline matched to a capture; compares live frames against it.
pub struct Baseline {
    rgb: RgbImage,
    /// Size of the live frame this was matched to.
    source: Size,
    lossy: bool,
    /// The capture has another size than the baseline: always "changed".
    size_changed: bool,
}

impl Baseline {
    fn needs_transform(&self) -> bool {
        self.lossy || (self.rgb.width(), self.rgb.height()) != (self.source.w, self.source.h)
    }

    /// The part of this baseline covering `r` (a sub-rect of the live frame),
    /// for a capture split across monitors. Resized or lossy baselines cannot
    /// be cut, so they only work when `r` is the whole frame.
    pub fn sub(self: &Arc<Self>, r: Rect) -> Result<Arc<Baseline>, SightError> {
        if self.size_changed {
            return Ok(self.clone()); // every part has the wrong size anyway
        }
        let whole =
            r.x == 0 && r.y == 0 && r.w as u32 == self.source.w && r.h as u32 == self.source.h;
        if whole {
            return Ok(self.clone());
        }
        if self.needs_transform() {
            return Err(bad(
                "this --baseline was scaled or saved as JPEG, so it cannot be used for a region that spans monitors or the screen edge".into(),
                "take the baseline as .png with --max-edge 0",
            ));
        }
        let (x, y, w, h) = (r.x as u32, r.y as u32, r.w as u32, r.h as u32);
        let rgb = image::imageops::crop_imm(&self.rgb, x, y, w, h).to_image();
        Ok(Arc::new(Baseline {
            rgb,
            source: Size { w, h },
            lossy: false,
            size_changed: false,
        }))
    }

    /// Number of pixels of a live BGRA frame (`w` x `h`) that differ from the
    /// baseline; `usize::MAX` when the frame has another size.
    pub fn diff(&self, bgra: &[u8], w: u32, h: u32) -> usize {
        if (w, h) != (self.source.w, self.source.h) || bgra.len() != (w * h * 4) as usize {
            return usize::MAX;
        }
        if !self.needs_transform() {
            return self
                .rgb
                .as_raw()
                .as_chunks::<3>()
                .0
                .iter()
                .zip(bgra.as_chunks::<4>().0)
                .filter(|(a, b)| a[0] != b[2] || a[1] != b[1] || a[2] != b[0])
                .count();
        }
        let mut rgba = bgra.to_vec();
        for px in rgba.as_chunks_mut::<4>().0 {
            px.swap(0, 2);
            px[3] = 255;
        }
        let img = RgbaImage::from_raw(w, h, rgba).expect("size checked above");
        let (ow, oh) = self.rgb.dimensions();
        let img = resize(&img, ow, oh);
        let mut live = DynamicImage::ImageRgba8(img).to_rgb8();
        if self.lossy {
            live = jpeg_roundtrip(live);
        }
        live.as_raw()
            .as_chunks::<3>()
            .0
            .iter()
            .zip(self.rgb.as_raw().as_chunks::<3>().0)
            .filter(|(a, b)| a != b)
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 3]) -> Vec<u8> {
        let mut v = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let [r, g, b] = f(x, y);
                v.extend_from_slice(&[b, g, r, 255]);
            }
        }
        v
    }

    fn rgb_of(bgra: &[u8], w: u32, h: u32) -> RgbImage {
        let mut v = Vec::new();
        for p in bgra.as_chunks::<4>().0 {
            v.extend_from_slice(&[p[2], p[1], p[0]]);
        }
        RgbImage::from_raw(w, h, v).unwrap()
    }

    fn pattern(x: u32, y: u32) -> [u8; 3] {
        [
            (x * 7 % 256) as u8,
            (y * 13 % 256) as u8,
            ((x ^ y) * 5 % 256) as u8,
        ]
    }

    fn base(img: RgbImage, lossy: bool, source: (u32, u32)) -> Baseline {
        Baseline {
            rgb: img,
            source: Size {
                w: source.0,
                h: source.1,
            },
            lossy,
            size_changed: false,
        }
    }

    #[test]
    fn lossless_baseline_counts_differing_pixels() {
        let (w, h) = (40, 30);
        let f = frame(w, h, pattern);
        let b = base(rgb_of(&f, w, h), false, (w, h));
        assert_eq!(b.diff(&f, w, h), 0);
        let mut g = f.clone();
        g[0] ^= 0xff; // pixel 0
        g[4 * 100 + 1] ^= 0xff; // pixel 100
        assert_eq!(b.diff(&g, w, h), 2);
        assert_eq!(b.diff(&f, w, h + 1), usize::MAX);
    }

    #[test]
    fn jpeg_baseline_matches_its_own_frame_exactly() {
        let (w, h) = (64, 48);
        let f = frame(w, h, pattern);
        let saved = jpeg_roundtrip(rgb_of(&f, w, h));
        let b = base(saved, true, (w, h));
        assert_eq!(b.diff(&f, w, h), 0);
        let mut g = f.clone();
        for p in g.as_chunks_mut::<4>().0.iter_mut().take(200) {
            p[0] = 255 - p[0];
            p[1] = 255 - p[1];
            p[2] = 255 - p[2];
        }
        assert!(b.diff(&g, w, h) > 50);
    }

    #[test]
    fn scaled_baseline_matches_the_same_downscale() {
        let (w, h) = (80, 60);
        let f = frame(w, h, pattern);
        let small = DynamicImage::ImageRgba8(resize(
            &RgbaImage::from_raw(w, h, {
                let mut v = f.clone();
                for p in v.as_chunks_mut::<4>().0 {
                    p.swap(0, 2);
                }
                v
            })
            .unwrap(),
            40,
            30,
        ))
        .to_rgb8();
        let b = base(small, false, (w, h));
        assert_eq!(b.diff(&f, w, h), 0);
    }

    #[test]
    fn sub_cuts_lossless_but_refuses_transformed() {
        let (w, h) = (20, 10);
        let f = frame(w, h, pattern);
        let b = Arc::new(base(rgb_of(&f, w, h), false, (w, h)));
        let r = Rect {
            x: 5,
            y: 2,
            w: 8,
            h: 4,
        };
        let part = b.sub(r).unwrap();
        let cut: Vec<u8> = frame(8, 4, |x, y| pattern(x + 5, y + 2));
        assert_eq!(part.diff(&cut, 8, 4), 0);
        let lossy = Arc::new(base(rgb_of(&f, w, h), true, (w, h)));
        assert!(lossy.sub(r).is_err());
        assert!(
            lossy
                .sub(Rect {
                    x: 0,
                    y: 0,
                    w: 20,
                    h: 10
                })
                .is_ok()
        );
    }

    fn loaded(w: u32, h: u32, meta: Option<Meta>) -> Loaded {
        Loaded {
            img: RgbImage::new(w, h),
            lossy: false,
            meta,
            path: PathBuf::from("b.png"),
        }
    }

    fn meta(target: TargetId, crop: Option<Rect>, w: u32, h: u32) -> Meta {
        Meta {
            v: 1,
            target,
            crop,
            source: Size { w, h },
            image: Size { w, h },
            format: "png".into(),
        }
    }

    #[test]
    fn size_difference_is_a_change_but_other_mismatches_are_errors() {
        let win = TargetId::Window { hwnd: 7 };
        let m = meta(win, None, 10, 8);
        let l = loaded(10, 8, Some(m));
        // Same size: fine, nothing to report.
        let (b, resized) = l.prepare(win, None, (10, 8)).unwrap();
        assert!(resized.is_none());
        assert_eq!(b.diff(&vec![0u8; 10 * 8 * 4], 10, 8), 0);
        // Bigger window: ok, reported, and every frame compares as changed.
        let (b, resized) = l.prepare(win, None, (14, 8)).unwrap();
        let (was, now) = resized.unwrap();
        assert_eq!((was.w, was.h, now.w, now.h), (10, 8, 14, 8));
        assert_eq!(b.diff(&vec![0u8; 14 * 8 * 4], 14, 8), usize::MAX);
        assert!(
            b.sub(Rect {
                x: 0,
                y: 0,
                w: 5,
                h: 8
            })
            .is_ok()
        );
        // Still errors: other window/target kind, other crop.
        assert!(
            l.prepare(TargetId::Window { hwnd: 8 }, None, (10, 8))
                .is_err()
        );
        let region = TargetId::Region {
            rect: Rect {
                x: 0,
                y: 0,
                w: 10,
                h: 8,
            },
        };
        assert!(l.prepare(region, None, (10, 8)).is_err());
        let c = Rect {
            x: 1,
            y: 1,
            w: 4,
            h: 4,
        };
        assert!(l.prepare(win, Some(c), (4, 4)).is_err());
        // No sidecar: a size difference stays an error, equal size is accepted.
        let foreign = loaded(10, 8, None);
        assert!(foreign.prepare(win, None, (14, 8)).is_err());
        assert!(foreign.prepare(win, None, (10, 8)).is_ok());
    }

    #[test]
    fn sidecar_path_replaces_the_extension() {
        assert_eq!(
            sidecar_path(Path::new("a/shot-1.png")),
            PathBuf::from("a/shot-1.sight.json")
        );
    }
}
