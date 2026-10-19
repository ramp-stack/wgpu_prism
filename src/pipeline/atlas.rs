use std::collections::HashMap;
use std::sync::Arc;

use image::RgbaImage;
use prism::drawable::text::{Character, Font};
use prism::hardware::Image;
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindingResource, Device,
    Extent3d, Origin3d, Queue, Sampler, TexelCopyBufferLayout, TexelCopyTextureInfo, TextureAspect,
    Texture, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages, TextureViewDescriptor,
};

#[derive(Default)]
pub struct ImageAtlas {
    images: Vec<(Image, Texture, Arc<BindGroup>)>,
    spare: Vec<(Texture, Arc<BindGroup>)>,
}
impl ImageAtlas {
    pub fn trim(&mut self) {
        self.spare.extend(self.images.extract_if(.., |(image, _, _)| {
            Arc::strong_count(image) == 1
        }).map(|(_, texture, group)| (texture, group)));
        let (mut count, mut bytes) = (0, 0u64);
        self.spare.reverse();
        self.spare.retain(|(texture, _)| {
            let size = u64::from(texture.width()) * u64::from(texture.height()) * 4;
            if count == 8 || bytes + size > 64 * 1024 * 1024 {return false}
            count += 1;
            bytes += size;
            true
        });
        self.spare.reverse();
    }

    pub fn get(&mut self, queue: &Queue, device: &Device, layout: &BindGroupLayout, sampler: &Sampler, image: &Image) -> Arc<BindGroup> {
        if let Some((_, _, group)) = self.images.iter().find(|(cached, _, _)| Arc::ptr_eq(cached, image)) {
            return group.clone()
        }
        let size = Extent3d {
            width: image.width(), height: image.height(), depth_or_array_layers: 1,
        };
        let reusable = self.spare.iter().rposition(|(texture, _)| {
            texture.size() == size && texture.format() == TextureFormat::Rgba8UnormSrgb
        });
        let (texture, group) = if let Some(index) = reusable {
            // Previous draws are submitted before trim; this queue's writes follow those draws.
            self.spare.remove(index)
        } else {
            let texture = device.create_texture(&TextureDescriptor {
                label: None, size, mip_level_count: 1, sample_count: 1,
                dimension: TextureDimension::D2, format: TextureFormat::Rgba8UnormSrgb,
                usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST, view_formats: &[],
            });
            let view = texture.create_view(&TextureViewDescriptor::default());
            let group = Arc::new(device.create_bind_group(&BindGroupDescriptor {
                label: None, layout,
                entries: &[
                    BindGroupEntry {binding: 0, resource: BindingResource::TextureView(&view)},
                    BindGroupEntry {binding: 1, resource: BindingResource::Sampler(sampler)},
                ],
            }));
            (texture, group)
        };
        queue.write_texture(
            TexelCopyTextureInfo {
                texture: &texture, mip_level: 0, origin: Origin3d::ZERO, aspect: TextureAspect::All,
            },
            image,
            TexelCopyBufferLayout {
                offset: 0, bytes_per_row: Some(4 * image.width()), rows_per_image: Some(image.height()),
            },
            size,
        );
        self.images.push((image.clone(), texture, group.clone()));
        group
    }
}

#[derive(Default)]
pub struct TextAtlas {fonts: Vec<(Font, Glyphs)>}
impl TextAtlas {
    pub fn trim(&mut self) {
        self.fonts.retain(|(font, _)| Arc::strong_count(&font.0) > 1);
        let (mut count, mut bytes) = self.fonts.iter().flat_map(|(_, glyphs)| glyphs.values())
            .fold((0, 0usize), |(count, bytes), (glyph, _)| {
                (count + 1, bytes + glyph.as_ref().map_or(0, |(image, _)| image.as_raw().len()))
            });
        for (_, glyphs) in &mut self.fonts {
            glyphs.retain(|_, (glyph, used)| {
                // Keep last frame's glyphs even when the visible text exceeds the cache budget.
                let keep = *used || (count <= 4096 && bytes <= 32 * 1024 * 1024);
                if !keep {
                    count -= 1;
                    bytes -= glyph.as_ref().map_or(0, |(image, _)| image.as_raw().len());
                }
                *used = false;
                keep
            });
        }
    }

    pub fn get(&mut self, character: &Character, scale: f32) -> Option<(Image, (f32, f32))> {
        let font = &character.2;
        let index = self.fonts.iter().position(|(cached, _)| Arc::ptr_eq(&cached.0, &font.0))
            .unwrap_or_else(|| {
                self.fonts.push((font.clone(), HashMap::new()));
                self.fonts.len() - 1
            });
        let color = character.3.is_none();
        let size = character.6 * scale;
        let (glyph, used) = self.fonts[index].1.entry((character.0.clone(), color, size.to_bits()))
            .or_insert_with(|| {
                let glyph = if color {
                    use swash::scale::{Render, ScaleContext, Source, StrikeWith, image::Content};
                    let font = swash::FontRef::from_index(font.bytes(), 0);
                    font.and_then(|font| {
                        let mut shaping = swash::shape::ShapeContext::new();
                        let mut shaper = shaping.builder(font).size(size).build();
                        shaper.add_str(&character.0);
                        let mut glyph = None;
                        shaper.shape_with(|cluster| {
                            if glyph.is_none() {glyph = cluster.glyphs.first().map(|glyph| glyph.id);}
                        });
                        let mut scaling = ScaleContext::new();
                        let mut scaler = scaling.builder(font).size(size).hint(false).build();
                        let render = Render::new(&[
                            Source::ColorBitmap(StrikeWith::BestFit), Source::ColorOutline(0), Source::Outline,
                        ]);
                        let image = render.render(&mut scaler, glyph?)?;
                        let (width, height) = (image.placement.width, image.placement.height);
                        if width == 0 || height == 0 {return None}
                        let offset = (image.placement.left as f32, -image.placement.top as f32);
                        let bytes = match image.content {
                            Content::Mask => image.data.into_iter().flat_map(|alpha| [0, 0, 0, alpha]).collect(),
                            Content::Color | Content::SubpixelMask => image.data,
                        };
                        RgbaImage::from_raw(width, height, bytes).map(|image| (Arc::new(image), offset))
                    })
                } else {
                    character.0.chars().next().and_then(|character| {
                        let (metrics, bytes) = font.rasterize(character, size);
                        if !bytes.iter().any(|alpha| *alpha != 0) {return None}
                        let offset = (metrics.xmin as f32, -(metrics.ymin as f32 + metrics.height as f32));
                        let bytes = bytes.into_iter().flat_map(|alpha| [0, 0, 0, alpha]).collect();
                        RgbaImage::from_raw(metrics.width as u32, metrics.height as u32, bytes)
                            .map(|image| (Arc::new(image), offset))
                    })
                };
                (glyph, false)
            });
        *used = true;
        glyph.clone()
    }
}

type Glyphs = HashMap<(String, bool, u32), (Option<(Image, (f32, f32))>, bool)>;
