use super::Color;
use std::ops::Deref;
use std::cell::RefCell;
use std::sync::{Mutex, Arc};
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hasher, Hash};
use lazy_static::lazy_static;
use unicode_segmentation::UnicodeSegmentation;
use swash::scale::{Render, Source, StrikeWith};

lazy_static! {
    static ref TEXT_LINES: Arc<Mutex<HashMap<u64, Vec<Line>>>> = Arc::default();

    pub(crate) static ref EMOJI_FONT: Font = Font::from_bytes(include_bytes!("../emoji.ttf")).expect("failed to load emoji font");
    static ref EMOJI_METRICS: Mutex<HashMap<(usize, String, u32), Option<SwashMetrics>>> = Mutex::default();
}

thread_local! {
    static SWASH: RefCell<(swash::scale::ScaleContext, swash::shape::ShapeContext)> =
        RefCell::new((swash::scale::ScaleContext::new(), swash::shape::ShapeContext::new()));
}

#[derive(Debug, Clone)]
pub struct Character(
    pub String,
    pub (f32, f32, f32, f32),
    pub Font,
    pub Option<Color>,
    pub f32,
    pub f32,
);

#[derive(Debug, Clone, Default)]
pub(crate) struct Line(pub f32, pub f32, pub Vec<Character>, pub Vec<(usize, f32)>);

impl Line {
    fn take(&mut self) -> Self {
        let l = Line(self.0, self.1, self.2.drain(..).collect(), std::mem::take(&mut self.3));
        self.0 = 0.0;
        self.1 = 0.0;
        l
    }
}

#[derive(Debug)]
pub struct FontInner {
    fontdue: fontdue::Font,
    bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct Font(pub Arc<FontInner>);

#[derive(Debug, Clone, Copy)]
pub(crate) struct SwashMetrics {
    pub xmin: f32,
    pub ymin: f32,
    pub width: f32,
    pub height: f32,
    pub advance_width: f32,
}

impl Font {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, &'static str> {
        let fontdue_font = fontdue::Font::from_bytes(
            bytes,
            fontdue::FontSettings {
                scale: 160.0,
                ..Default::default()
            },
        )?;

        Ok(Font(Arc::new(FontInner {
            fontdue: fontdue_font,
            bytes: bytes.to_vec(),
        })))
    }

    pub(crate) fn swash_font(&self) -> swash::FontRef<'_> {
        swash::FontRef::from_index(&self.0.bytes, 0)
            .expect("invalid font data")
    }

    pub(crate) fn swash_metrics(&self, grapheme: &str, size: f32) -> Option<SwashMetrics> {
        let key = (self.file_hash(), grapheme.to_string(), size.to_bits());
        if let Some(metrics) = EMOJI_METRICS.lock().unwrap().get(&key) {
            return *metrics;
        }

        let metrics = SWASH.with(|contexts| {
            let mut contexts = contexts.borrow_mut();
            let (scale_context, shape_context) = &mut *contexts;
            self.measure_grapheme(scale_context, shape_context, grapheme, size)
        });

        EMOJI_METRICS.lock().unwrap().insert(key, metrics);
        metrics
    }

    fn measure_grapheme(&self,
        scale_context: &mut swash::scale::ScaleContext,
        shape_context: &mut swash::shape::ShapeContext,
        grapheme: &str, size: f32,
    ) -> Option<SwashMetrics> {
        let mut shaper = shape_context.builder(self.swash_font()).size(size).build();
        shaper.add_str(grapheme);

        let mut glyph_id = None;
        let mut advance_width = 0.0_f32;

        shaper.shape_with(|cluster| cluster.glyphs.iter().for_each(|glyph| {
            if glyph_id.is_none() { glyph_id = Some(glyph.id); }
            advance_width += glyph.advance;
        }));

        let mut scaler = scale_context
            .builder(self.swash_font())
            .size(size).hint(false).build();

        if let Some(outline) = scaler.scale_color_outline(glyph_id.unwrap()) {
            let bounds = outline.bounds();
            let (width, height) = (bounds.max.x - bounds.min.x, bounds.max.y - bounds.min.y);
            if width > 0.0 && height > 0.0 {
                return Some(SwashMetrics { xmin: bounds.min.x, ymin: bounds.min.y, width, height, advance_width });
            }
        }

        let image = Render::new(&[
            Source::ColorBitmap(StrikeWith::BestFit),
            Source::ColorOutline(0),
            Source::Outline,
        ]).render(&mut scaler, glyph_id.unwrap())?;

        (image.placement.width > 0 && image.placement.height > 0).then(|| {
            let height = image.placement.height as f32;

            SwashMetrics {
                xmin: image.placement.left as f32,
                ymin: image.placement.top as f32 - height,
                width: image.placement.width as f32,
                height, advance_width,
            }
        })
    }
}

impl Deref for Font {
    type Target = fontdue::Font;

    fn deref(&self) -> &Self::Target {
        &self.0.fontdue
    }
}

impl PartialEq for Font {
    fn eq(&self, other: &Font) -> bool {
        self.file_hash() == other.file_hash()
    }
}

impl Hash for Font {
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        self.file_hash().hash(hasher)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Hash)]
pub enum Align {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    pub text: String,
    pub font_size: f32,
    pub line_height: Option<f32>,
    pub font: Font,
    pub color: Color,
    pub kerning: f32,
}

impl Hash for Span {
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        self.text.hash(hasher);
        self.font_size.to_bits().hash(hasher);

        if let Some(l) = self.line_height {
            l.to_bits().hash(hasher);
        }

        self.font.hash(hasher);
        self.color.hash(hasher);
        self.kerning.to_bits().hash(hasher);
    }
}

impl Span {
    pub fn new(
        text: String,
        font_size: f32,
        line_height: Option<f32>,
        font: Font,
        color: Color,
        kerning: f32,
    ) -> Self {
        Span {
            text,
            font_size,
            line_height,
            font,
            color,
            kerning,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Text {
    pub spans: Vec<Span>,
    pub width: Option<f32>,
    pub align: Align,
    pub cursor: Option<usize>,
    pub max_lines: Option<u32>,
}

impl Hash for Text {
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        self.spans.hash(hasher);
        if let Some(w) = self.width { w.to_bits().hash(hasher); }
        self.align.hash(hasher);
        if let Some(c) = self.cursor { c.hash(hasher); }
        if let Some(max) = self.max_lines { max.hash(hasher); }
    }
}

impl Text {
    pub fn new(
        text: &str,
        font: Font,
        font_size: f32,
        color: Color,
        align: Align,
    ) -> Self {
        Text {
            spans: vec![Span::new(text.to_string(), font_size, None, font, color, 0.0)],
            width: None,
            align,
            cursor: None,
            max_lines: None,
        }
    }

    pub fn scale(&mut self, scale: f32) {
        self.width = self.width.map(|w| scale*w);
        self.spans.iter_mut().for_each(|span| {
            span.font_size *= scale;
            span.line_height = span.line_height.map(|l| scale*l);
            span.kerning *= scale;
        });
    }

    pub fn size(&self) -> (f32, f32) {
        let (w, h) = self.lines().iter().fold((0.0_f32, 0.0_f32), |(w, h), line| {
            (w.max(line.0), h + line.1)
        });

        if self.ends_with_newline() && 
        self.max_lines.is_none_or(|max| self.lines().len() < max as usize) {
            let h = h + self.spans.iter().rev().find(|s| !s.text.is_empty()).map_or(0.0, |s| {
                s.line_height.unwrap_or_else(|| {
                    s.font.horizontal_line_metrics(s.font_size).map_or(0.0, |m| m.new_line_size)
                })
            });

            (w, h)
        } else {(w, h)}
    }

    pub fn cursor_position(&self) -> (f32, f32) {
        self.cursor.map(|cursor| {
            let (_, x, top) = self.locate(&self.lines(), cursor);
            (x, top)
        }).unwrap_or((0.0, 0.0))
    }

    fn locate(&self, lines: &[Line], cursor: usize) -> (usize, f32, f32) {
        let cursor = cursor.min(self.text_len());

        if cursor == self.text_len() && self.ends_with_newline() {
            return (lines.len(), self.empty_line_x(), lines.iter().map(|l| l.1).sum());
        }

        let mut top = 0.0;
        let mut found = None;
        for (i, line) in lines.iter().enumerate() {
            for &(byte, x) in &line.3 {
                if byte > cursor { return found.unwrap_or((i, x, top)); }
                found = Some((i, x, top));
            }
            top += line.1;
        }

        found.unwrap_or((0, self.empty_line_x(), 0.0))
    }

    pub fn cursor_click(&mut self, x: f32, y: f32) {
        let lines = self.lines();
        let height: f32 = lines.iter().map(|l| l.1).sum();

        if lines.is_empty() || (y >= height && self.ends_with_newline()) {
            self.cursor = Some(self.text_len());
            return;
        }

        let mut top = 0.0;
        let mut target = &lines[lines.len() - 1];
        for line in &lines {
            if y < top + line.1 { target = line; break; }
            top += line.1;
        }

        if let Some(byte) = target.3.iter()
        .min_by(|a, b| (a.1 - x).abs().total_cmp(&(b.1 - x).abs()))
        .map(|&(byte, _)| byte) {
            self.cursor = Some(byte);
        }
    }

    pub fn text_len(&self) -> usize {
        self.spans.iter().map(|s| s.text.len()).sum()
    }

    fn ends_with_newline(&self) -> bool {
        self.spans.iter().rev().find(|s| !s.text.is_empty())
            .is_some_and(|s| s.text.ends_with('\n'))
    }

    fn empty_line_x(&self) -> f32 {
        match self.align {
            Align::Left => 0.0,
            Align::Center => self.width.map_or(0.0, |w| w / 2.0),
            Align::Right => self.width.unwrap_or(0.0),
        }
    }

    pub fn len(&self) -> usize { self.lines().len() }
    pub fn is_empty(&self) -> bool { self.len() == 0 }

    pub(crate) fn lines(&self) -> Vec<Line> {
        let mut hasher = DefaultHasher::new();
        self.spans.hash(&mut hasher);
        self.width.map(f32::to_bits).hash(&mut hasher);
        self.align.hash(&mut hasher);
        self.max_lines.hash(&mut hasher);

        TEXT_LINES.lock().unwrap().entry(hasher.finish())
            .or_insert_with(|| self.inner_lines()).clone()
    }

    pub(crate) fn inner_lines(&self) -> Vec<Line> {
        let mut lines = Vec::new();
        let mut line = Line::default();
        let (mut line_start, mut line_end) = (0, 0);

        let push = |lines: &mut Vec<Line>, line: &mut Line, lh: f32,
                    start: &mut usize, end: usize, next: usize| {
            if line.2.is_empty() {
                line.1 = line.1.max(lh);
                line.3 = vec![(*start, 0.0)];
            } else {
                line.3.push((end, line.0));
            }

            line.2.iter_mut().for_each(|ch| ch.1 .1 += line.1);
            lines.push(line.take());
            *start = next;
        };

        let (mut span_start, mut soft_wrapped) = (0, false);

        for s in &self.spans {
            let lm = s.font.horizontal_line_metrics(s.font_size).unwrap();
            let lh = s.line_height.unwrap_or(lm.new_line_size);
            let mut raw_start = span_start;

            for (i, raw) in s.text.split('\n').enumerate() {
                if i > 0 {
                    push(&mut lines, &mut line, lh, &mut line_start, line_end, raw_start);
                    soft_wrapped = false;
                }

                let mut word_start = raw_start;

                for word in raw.split_inclusive(char::is_whitespace) {
                    let mut ww = 0.0;

                    let glyphs: Vec<_> = UnicodeSegmentation::grapheme_indices(word, true).map(|(gi, g)| {
                            let emoji = is_emoji_grapheme(g);
                            let font = if emoji { EMOJI_FONT.clone() } else { s.font.clone() };

                            let (xmin, ymin, w, h, aw) = if emoji {
                                font.swash_metrics(g, s.font_size)
                                    .map(|m| (m.xmin, m.ymin, m.width, m.height, m.advance_width))
                                    .unwrap_or_default()
                            } else {
                                let (mut aw, mut xmin, mut ymin, mut xmax, mut ymax) = (0.0_f32, 0.0, 0.0, 0.0, 0.0);

                                g.chars().enumerate().for_each(|(i, c)| {
                                    let m = font.metrics(c, s.font_size);
                                    let x = aw + m.bounds.xmin;
                                    let y = m.bounds.ymin;

                                    xmin = if i == 0 { x } else { xmin.min(x) };
                                    ymin = if i == 0 { y } else { ymin.min(y) };
                                    xmax = if i == 0 { x + m.bounds.width } else { xmax.max(x + m.bounds.width) };
                                    ymax = if i == 0 { y + m.bounds.height } else { ymax.max(y + m.bounds.height) };

                                    aw += m.advance_width;
                                });

                                (xmin, ymin, xmax - xmin, ymax - ymin, aw)
                            };

                            ww += aw + s.kerning;

                            (
                                word_start + gi, g.len(), g.to_string(), font,
                                (xmin, ymin, w, h), aw,
                                g.chars().all(char::is_whitespace), emoji,
                            )
                        }).collect();

                    if let Some(wmax) = self.width
                        && ww < wmax
                        && line.0 + ww > wmax
                        && !line.2.is_empty()
                    {
                        soft_wrapped = true;
                        push(&mut lines, &mut line, lh, &mut line_start, line_end, word_start);
                    }

                    for (byte, len, g, font, (xmin, ymin, w, h), aw, whitespace, emoji) in glyphs {
                        if let Some(wmax) = self.width {
                            if line.0 + aw + s.kerning > wmax && !line.2.is_empty() {
                                push(&mut lines, &mut line, lh, &mut line_start, line_end, byte);
                                soft_wrapped = true;
                            }

                            if soft_wrapped && line.2.is_empty() && whitespace {
                                line_start = byte + len;
                                continue;
                            }
                        }

                        let y = lines.iter().map(|l| l.1).sum::<f32>() - ymin - h + lm.descent;

                        line.3.push((byte, line.0));
                        line_end = byte + len;

                        line.2.push(Character(
                            g,
                            (line.0 + xmin, y, w, h),
                            font,
                            (!emoji).then_some(s.color),
                            lh,
                            aw,
                        ));

                        line.0 += aw + s.kerning;
                        line.1 = line.1.max(lh);
                    }

                    word_start += word.len();
                }

                raw_start += raw.len() + 1;
            }

            span_start += s.text.len();
        }

        if !line.2.is_empty() {
            push(&mut lines, &mut line, 0.0, &mut line_start, line_end, line_end);
        }

        // Remove duplicate caret stops at wrapped boundaries.
        for i in 1..lines.len() {
            let next = lines[i].3.first().map(|c| c.0);
            let prev = &mut lines[i - 1].3;
            if prev.len() > 1 && prev.last().map(|c| c.0) == next { prev.pop(); }
        }

        // Align lines.
        for line in &mut lines {
            let offset = match self.align {
                Align::Left => 0.0,
                Align::Center => self.width.map_or(0.0, |w| (w - line.0) / 2.0),
                Align::Right => self.width.map_or(0.0, |w| w - line.0),
            };

            line.2.iter_mut().for_each(|ch| ch.1 .0 += offset);
            line.3.iter_mut().for_each(|stop| stop.1 += offset);
            line.0 += offset;
        }

        // Truncate and append ellipsis.
        if let Some(max) = self.max_lines {
            let overflow = lines.len() > max as usize;
            lines.truncate(max as usize);

            if overflow {
                let y = lines.iter().map(|l| l.1).sum::<f32>();

                if let Some(last) = lines.last_mut() {
                    last.2.truncate(last.2.len().saturating_sub(3));
                    last.3.truncate(last.2.len() + 1);

                    let s = &self.spans[0];
                    let lm = s.font.horizontal_line_metrics(s.font_size).unwrap();
                    let lh = s.line_height.unwrap_or(lm.new_line_size);
                    let mut x = last.2.last().map(|g| g.1 .0 + g.5).unwrap_or(0.0);

                    for c in "...".chars() {
                        let m = s.font.metrics(c, s.font_size);
                        let aw = m.advance_width + s.kerning;

                        last.2.push(Character(
                            c.to_string(),
                            (
                                x + m.bounds.xmin,
                                y - m.bounds.ymin - m.bounds.height + lm.descent * s.font_size,
                                m.bounds.width,
                                m.bounds.height,
                            ),
                            s.font.clone(),
                            Some(s.color),
                            lh,
                            aw,
                        ));

                        x += aw;
                        last.0 += aw;
                        last.1 = last.1.max(lh);
                    }
                }
            }
        }

        lines
    }

}

pub fn is_emoji_grapheme(g: &str) -> bool {
    g.chars().any(|c| {
        matches!(
            c as u32,
            0x1F000..=0x1FAFF |
            0x2600..=0x27BF |
            0x2300..=0x23FF |
            0xFE0F | 0x20E3 |
            0x2B50 | 0x2B55 | 0x2B1B | 0x2B1C | 0x25FD | 0x25FE
        )
    })
}