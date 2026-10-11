//! Text on the Switch screen, read with macOS Vision from the pixelPeek JPEG.
//!
//! The Switch has no accessibility tree, so this is the closest thing to
//! iphone-use's element list: every line of text with its box in the
//! 1280x720 touch space, so a label can be tapped or waited for.

use anyhow::Result;
use serde_json::{json, Value};

#[derive(Clone, Debug)]
pub struct Text {
    pub text: String,
    /// Top-left corner and size, in screen pixels.
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub conf: f32,
}

impl Text {
    pub fn center(&self) -> (u32, u32) {
        (self.x + self.w / 2, self.y + self.h / 2)
    }

    pub fn json(&self, i: usize) -> Value {
        let (cx, cy) = self.center();
        json!({
            "i": i, "text": self.text, "x": cx, "y": cy,
            "box": [self.x, self.y, self.w, self.h],
            "conf": (self.conf as f64 * 100.0).round() / 100.0,
        })
    }
}

/// Lines sorted top to bottom, left to right. An item joins the current row
/// when its center is within half the smaller line height of the row's first
/// item, so a tall title cannot swallow the lines around it.
pub fn read(jpeg: &[u8]) -> Result<Vec<Text>> {
    let mut items = recognize(jpeg)?;
    items.sort_by_key(|t| (t.y + t.h / 2, t.x));
    let mut rows: Vec<Vec<Text>> = Vec::new();
    for t in items {
        let cy = t.y + t.h / 2;
        match rows.last_mut() {
            Some(row) if cy.abs_diff(row[0].y + row[0].h / 2) <= row[0].h.min(t.h) / 2 => row.push(t),
            _ => rows.push(vec![t]),
        }
    }
    Ok(rows
        .into_iter()
        .flat_map(|mut row| {
            row.sort_by_key(|t| t.x);
            row
        })
        .collect())
}

fn norm(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).flat_map(char::to_lowercase).collect()
}

/// Exact matches first (ignoring case and spaces), then substring matches.
pub fn find<'a>(items: &'a [Text], label: &str) -> Vec<(usize, &'a Text)> {
    let want = norm(label);
    if want.is_empty() {
        return Vec::new();
    }
    let exact: Vec<_> = items.iter().enumerate().filter(|(_, t)| norm(&t.text) == want).collect();
    if !exact.is_empty() {
        return exact;
    }
    items.iter().enumerate().filter(|(_, t)| norm(&t.text).contains(&want)).collect()
}

/// What to suggest when a label is not on screen: items sharing a character with it.
pub fn near<'a>(items: &'a [Text], label: &str) -> Vec<&'a Text> {
    let want = norm(label);
    let mut scored: Vec<(usize, &Text)> = items
        .iter()
        .map(|t| (want.chars().filter(|c| norm(&t.text).contains(*c)).count(), t))
        .filter(|(n, _)| *n > 0)
        .collect();
    scored.sort_by_key(|s| std::cmp::Reverse(s.0));
    let mut out: Vec<&Text> = Vec::new();
    for (_, t) in scored {
        if out.len() < 5 && !out.iter().any(|o| o.text == t.text) {
            out.push(t);
        }
    }
    out
}

#[cfg(target_os = "macos")]
fn recognize(jpeg: &[u8]) -> Result<Vec<Text>> {
    use anyhow::anyhow;
    use objc2::rc::autoreleasepool;
    use objc2::AnyThread;
    use objc2_foundation::{NSArray, NSData, NSDictionary, NSString};
    use objc2_vision::{
        VNImageRequestHandler, VNRecognizeTextRequest, VNRequest, VNRequestTextRecognitionLevel,
    };

    const W: f64 = 1280.0;
    const H: f64 = 720.0;
    autoreleasepool(|_| {
        let data = NSData::with_bytes(jpeg);
        let handler =
            VNImageRequestHandler::initWithData_options(VNImageRequestHandler::alloc(), &data, &NSDictionary::new());
        let request = unsafe { VNRecognizeTextRequest::init(VNRecognizeTextRequest::alloc()) };
        request.setRecognitionLevel(VNRequestTextRecognitionLevel::Accurate);
        let langs: Vec<_> = ["zh-Hans", "zh-Hant", "ja-JP", "en-US"].iter().map(|l| NSString::from_str(l)).collect();
        request.setRecognitionLanguages(&NSArray::from_retained_slice(&langs));
        request.setUsesLanguageCorrection(true);
        let as_request: &VNRequest = &request;
        handler
            .performRequests_error(&NSArray::from_slice(&[as_request]))
            .map_err(|e| anyhow!("Vision text recognition failed: {}", e.localizedDescription()))?;

        let mut out = Vec::new();
        for obs in request.results().map(|r| r.to_vec()).unwrap_or_default() {
            let Some(best) = obs.topCandidates(1).firstObject() else { continue };
            let text = best.string().to_string();
            if text.trim().is_empty() {
                continue;
            }
            // Normalized, origin bottom-left.
            let b = unsafe { obs.boundingBox() };
            let px = |v: f64, max: f64| (v.clamp(0.0, 1.0) * max).round() as u32;
            out.push(Text {
                text,
                x: px(b.origin.x, W),
                y: px(1.0 - b.origin.y - b.size.height, H),
                w: px(b.size.width, W),
                h: px(b.size.height, H),
                conf: best.confidence(),
            });
        }
        Ok(out)
    })
}

#[cfg(not(target_os = "macos"))]
fn recognize(_jpeg: &[u8]) -> Result<Vec<Text>> {
    anyhow::bail!("reading screen text uses macOS Vision; on this platform use `switch-use screen` and look at the image")
}
