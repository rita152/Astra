use anyhow::anyhow;
use cocoa::appkit::CGFloat;
use collections::{HashMap, HashSet};
use core_foundation::{
    array::{CFArray, CFArrayRef},
    attributed_string::CFMutableAttributedString,
    base::{CFHash, CFRange, CFType, TCFType},
    dictionary::CFDictionary,
    number::CFNumber,
    string::CFString,
};
use core_graphics::{
    base::{CGGlyph, kCGImageAlphaPremultipliedLast},
    color_space::CGColorSpace,
    context::{CGContext, CGTextDrawingMode},
    display::CGPoint,
};
use core_text::{
    font::CTFont,
    font_collection::CTFontCollectionRef,
    font_descriptor::{
        CTFontDescriptor, kCTFontSlantTrait, kCTFontSymbolicTrait, kCTFontWeightTrait,
        kCTFontWidthTrait,
    },
    line::CTLine,
    string_attributes::kCTFontAttributeName,
};
use font_kit::{
    font::Font as FontKitFont,
    handle::Handle,
    hinting::HintingOptions,
    metrics::Metrics,
    properties::{Style as FontkitStyle, Weight as FontkitWeight},
    source::SystemSource,
    sources::mem::MemSource,
};
use gpui::{
    Bounds, DevicePixels, Font, FontFallbacks, FontFeatures, FontId, FontMetrics, FontRun,
    FontStyle, FontWeight, GlyphId, Hsla, LineLayout, Pixels, PlatformTextSystem,
    RenderGlyphParams, Result, Rgba, SUBPIXEL_VARIANTS_X, ShapedGlyph, ShapedRun, SharedString,
    Size, TextRenderingMode, point, px, size, swap_rgba_pa_to_bgra,
};
use parking_lot::{RwLock, RwLockUpgradableReadGuard};
use pathfinder_geometry::{
    rect::{RectF, RectI},
    transform2d::Transform2F,
    vector::Vector2F,
};
use smallvec::SmallVec;
use std::{borrow::Cow, char, convert::TryFrom, sync::Arc, sync::OnceLock};
use unicode_segmentation::UnicodeSegmentation;

use crate::open_type::apply_features_and_fallbacks;

#[allow(non_upper_case_globals)]
const kCGImageAlphaOnly: u32 = 7;

/// macOS text system using CoreText for font shaping.
pub struct MacTextSystem(RwLock<MacTextSystemState>);

#[derive(Clone, PartialEq, Eq, Hash)]
struct FontKey {
    font_family: SharedString,
    font_features: FontFeatures,
    font_fallbacks: Option<FontFallbacks>,
}

// A PostScript name does not identify a variable-font instance, optical size,
// fallback cascade or OpenType features. Keep CoreText's complete identity.
#[derive(Clone, PartialEq, Eq)]
struct NativeFontKey(CTFont, bool);

impl std::hash::Hash for NativeFontKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write_usize(unsafe { CFHash(self.0.as_CFTypeRef()) } as usize);
        state.write_u8(self.1 as u8);
    }
}

struct MacTextSystemState {
    memory_source: MemSource,
    system_source: SystemSource,
    fonts: Vec<FontKitFont>,
    font_selections: HashMap<Font, FontId>,
    font_requests: HashMap<FontId, Font>,
    synthetic_bold_fonts: HashSet<FontId>,
    font_ids_by_native_font: HashMap<NativeFontKey, FontId>,
    font_ids_by_font_key: HashMap<FontKey, SmallVec<[FontId; 4]>>,
    postscript_names_by_font_id: HashMap<FontId, String>,
}

impl MacTextSystem {
    /// Create a new MacTextSystem.
    pub fn new() -> Self {
        Self(RwLock::new(MacTextSystemState {
            memory_source: MemSource::empty(),
            system_source: SystemSource::new(),
            fonts: Vec::new(),
            font_selections: HashMap::default(),
            font_requests: HashMap::default(),
            synthetic_bold_fonts: HashSet::default(),
            font_ids_by_native_font: HashMap::default(),
            font_ids_by_font_key: HashMap::default(),
            postscript_names_by_font_id: HashMap::default(),
        }))
    }
}

impl Default for MacTextSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl PlatformTextSystem for MacTextSystem {
    fn add_fonts(&self, fonts: Vec<Cow<'static, [u8]>>) -> Result<()> {
        self.0.write().add_fonts(fonts)
    }

    fn all_font_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        let collection = core_text::font_collection::create_for_all_families();
        // NOTE: We intentionally avoid using `collection.get_descriptors()` here because
        // it has a memory leak bug in core-text v21.0.0. The upstream code uses
        // `wrap_under_get_rule` but `CTFontCollectionCreateMatchingFontDescriptors`
        // follows the Create Rule (caller owns the result), so it should use
        // `wrap_under_create_rule`. We call the function directly with correct memory management.
        unsafe extern "C" {
            fn CTFontCollectionCreateMatchingFontDescriptors(
                collection: CTFontCollectionRef,
            ) -> CFArrayRef;
        }
        let descriptors: Option<CFArray<CTFontDescriptor>> = unsafe {
            let array_ref =
                CTFontCollectionCreateMatchingFontDescriptors(collection.as_concrete_TypeRef());
            if array_ref.is_null() {
                None
            } else {
                Some(CFArray::wrap_under_create_rule(array_ref))
            }
        };
        let Some(descriptors) = descriptors else {
            return names;
        };
        for descriptor in descriptors.into_iter() {
            names.extend(lenient_font_attributes::family_name(&descriptor));
        }
        if let Ok(fonts_in_memory) = self.0.read().memory_source.all_families() {
            names.extend(fonts_in_memory);
        }
        names
    }

    fn font_id(&self, font: &Font) -> Result<FontId> {
        let lock = self.0.upgradable_read();
        if let Some(font_id) = lock.font_selections.get(font) {
            Ok(*font_id)
        } else {
            RwLockUpgradableReadGuard::upgrade(lock).select_font(font)
        }
    }

    fn font_metrics(&self, font_id: FontId) -> FontMetrics {
        font_kit_metrics_to_metrics(self.0.read().fonts[font_id.0].metrics())
    }

    fn typographic_bounds(&self, font_id: FontId, glyph_id: GlyphId) -> Result<Bounds<f32>> {
        Ok(bounds_from_rect(
            self.0.read().fonts[font_id.0].typographic_bounds(glyph_id.0)?,
        ))
    }

    fn advance(&self, font_id: FontId, glyph_id: GlyphId) -> Result<Size<f32>> {
        self.0.read().advance(font_id, glyph_id)
    }

    fn glyph_for_char(&self, font_id: FontId, ch: char) -> Option<GlyphId> {
        self.0.read().glyph_for_char(font_id, ch)
    }

    fn glyph_raster_bounds(&self, params: &RenderGlyphParams) -> Result<Bounds<DevicePixels>> {
        self.0.read().raster_bounds(params)
    }

    fn rasterize_glyph(
        &self,
        glyph_id: &RenderGlyphParams,
        raster_bounds: Bounds<DevicePixels>,
    ) -> Result<(Size<DevicePixels>, Vec<u8>)> {
        self.0.read().rasterize_glyph(glyph_id, raster_bounds)
    }

    fn layout_line(&self, text: &str, font_size: Pixels, font_runs: &[FontRun]) -> LineLayout {
        self.0.write().layout_line(text, font_size, font_runs)
    }

    fn recommended_rendering_mode(
        &self,
        _font_id: FontId,
        _font_size: Pixels,
    ) -> TextRenderingMode {
        TextRenderingMode::Grayscale
    }

    fn glyph_dilation_for_color(&self, color: Hsla) -> u8 {
        // When font smoothing is enabled, CoreGraphics thickens glyph strokes by an amount that
        // depends on the foreground color's luminance. We replicate the logic used by CoreGraphics
        // to select between the different levels of dilation.
        if !font_smoothing_allowed_by_user() {
            return 0;
        }
        let rgba: Rgba = color.into();
        let luminance = 0.2126 * rgba.r + 0.7152 * rgba.g + 0.0722 * rgba.b;
        let level = ((4.0 * luminance) + 0.5).floor() as i32;
        level.clamp(0, 4) as u8
    }
}

fn font_smoothing_allowed_by_user() -> bool {
    static ALLOWED: OnceLock<bool> = OnceLock::new();
    *ALLOWED.get_or_init(|| {
        use core_foundation_sys::preferences::{
            CFPreferencesCopyAppValue, kCFPreferencesCurrentApplication,
        };

        let key = CFString::new("AppleFontSmoothing");
        let value_ref = unsafe {
            CFPreferencesCopyAppValue(key.as_concrete_TypeRef(), kCFPreferencesCurrentApplication)
        };
        if value_ref.is_null() {
            return true;
        }
        let value = unsafe { CFType::wrap_under_create_rule(value_ref) };
        let Some(number) = value.downcast_into::<CFNumber>() else {
            return true;
        };
        // Only an explicit value of `0` means that font smoothing is disabled.
        number.to_i64() != Some(0)
    })
}

impl MacTextSystemState {
    fn select_font(&mut self, font: &Font) -> Result<FontId> {
        if let Some(id) = self.font_selections.get(font) {
            return Ok(*id);
        }
        if font.family.as_ref() == ".SystemUIFont" {
            let mut native = chromium_system_font(font.weight, font.style);
            apply_features_and_fallbacks(&mut native, &font.features, font.fallbacks.as_ref())?;
            let font_id = self.id_for_native_font(native.native_font(), false);
            self.font_selections.insert(font.clone(), font_id);
            self.font_requests.insert(font_id, font.clone());
            return Ok(font_id);
        }
        let font_key = FontKey {
            font_family: font.family.clone(),
            font_features: font.features.clone(),
            font_fallbacks: font.fallbacks.clone(),
        };
        let candidates = if let Some(font_ids) = self.font_ids_by_font_key.get(&font_key) {
            font_ids.as_slice()
        } else {
            let font_ids =
                self.load_family(&font.family, &font.features, font.fallbacks.as_ref())?;
            self.font_ids_by_font_key.insert(font_key.clone(), font_ids);
            self.font_ids_by_font_key[&font_key].as_ref()
        };

        let candidate_properties = candidates
            .iter()
            .map(|font_id| {
                use core_text::font_descriptor::TraitAccessors;
                let font = &self.fonts[font_id.0];
                let mut properties = font.properties();
                properties.weight = FontkitWeight(css_weight_from_core_text(
                    font.native_font().all_traits().normalized_weight(),
                ));
                properties
            })
            .collect::<SmallVec<[_; 4]>>();

        let ix = font_kit::matching::find_best_match(
            &candidate_properties,
            &font_kit::properties::Properties {
                style: fontkit_style(font.style),
                weight: fontkit_weight(font.weight),
                stretch: Default::default(),
            },
        )?;

        let mut font_id = candidates[ix];
        if font.weight.0 >= 600.0
            && (self.fonts[font_id.0].native_font().symbolic_traits()
                & core_text::font_descriptor::kCTFontBoldTrait)
                == 0
        {
            font_id = self.id_for_native_font(self.fonts[font_id.0].native_font(), true);
        }
        self.font_selections.insert(font.clone(), font_id);
        self.font_requests.insert(font_id, font.clone());
        Ok(font_id)
    }

    fn add_fonts(&mut self, fonts: Vec<Cow<'static, [u8]>>) -> Result<()> {
        let fonts = fonts
            .into_iter()
            .map(|bytes| match bytes {
                Cow::Borrowed(embedded_font) => {
                    let data_provider = unsafe {
                        core_graphics::data_provider::CGDataProvider::from_slice(embedded_font)
                    };
                    let font = core_graphics::font::CGFont::from_data_provider(data_provider)
                        .map_err(|()| anyhow!("Could not load an embedded font."))?;
                    let font = font_kit::loaders::core_text::Font::from_core_graphics_font(font);
                    Ok(Handle::from_native(&font))
                }
                Cow::Owned(bytes) => {
                    // CoreGraphics supports native WOFF2 resources too. Keep
                    // owned data on the same path as embedded fonts; font-kit's
                    // file-format probe rejects WOFF2 before CoreText sees it.
                    let provider =
                        core_graphics::data_provider::CGDataProvider::from_buffer(Arc::new(bytes));
                    let font = core_graphics::font::CGFont::from_data_provider(provider)
                        .map_err(|()| anyhow!("Could not load an owned font."))?;
                    let font = FontKitFont::from_core_graphics_font(font);
                    Ok(Handle::from_native(&font))
                }
            })
            .collect::<Result<Vec<_>>>()?;
        self.memory_source.add_fonts(fonts.into_iter())?;
        Ok(())
    }

    fn load_family(
        &mut self,
        name: &str,
        features: &FontFeatures,
        fallbacks: Option<&FontFallbacks>,
    ) -> Result<SmallVec<[FontId; 4]>> {
        let name = gpui::font_name_with_fallbacks(name, ".AppleSystemUIFont");

        let mut font_ids = SmallVec::new();
        let mut postscript_names_seen = HashSet::default();
        let family = self
            .memory_source
            .select_family_by_name(name)
            .or_else(|_| self.system_source.select_family_by_name(name))?;
        for font in family.fonts() {
            let mut font = font.load()?;

            apply_features_and_fallbacks(&mut font, features, fallbacks)?;
            // This block contains a precautionary fix to guard against loading fonts
            // that might cause panics due to `.unwrap()`s up the chain.
            {
                // We use the 'm' character for text measurements in various spots
                // (e.g., the editor). However, at time of writing some of those usages
                // will panic if the font has no 'm' glyph.
                //
                // Therefore, we check up front that the font has the necessary glyph.
                let has_m_glyph = font.glyph_for_char('m').is_some();

                // HACK: The 'Segoe Fluent Icons' font does not have an 'm' glyph,
                // but we need to be able to load it for rendering Windows icons in
                // the Storybook (on macOS).
                let is_segoe_fluent_icons = font.full_name() == "Segoe Fluent Icons";

                if !has_m_glyph && !is_segoe_fluent_icons {
                    // I spent far too long trying to track down why a font missing the 'm'
                    // character wasn't loading. This log statement will hopefully save
                    // someone else from suffering the same fate.
                    log::warn!(
                        "font '{}' has no 'm' character and was not loaded",
                        font.full_name()
                    );
                    continue;
                }
            }

            // We've seen a number of panics in production caused by calling font.properties()
            // which unwraps a downcast to CFNumber. This is an attempt to avoid the panic,
            // and to try and identify the incalcitrant font.
            let traits = font.native_font().all_traits();
            if unsafe {
                !(traits
                    .get(kCTFontSymbolicTrait)
                    .downcast::<CFNumber>()
                    .is_some()
                    && traits
                        .get(kCTFontWidthTrait)
                        .downcast::<CFNumber>()
                        .is_some()
                    && traits
                        .get(kCTFontWeightTrait)
                        .downcast::<CFNumber>()
                        .is_some()
                    && traits
                        .get(kCTFontSlantTrait)
                        .downcast::<CFNumber>()
                        .is_some())
            } {
                log::error!(
                    "Failed to read traits for font {:?} (PostScript name {:?})",
                    font.full_name(),
                    font.postscript_name(),
                );
                continue;
            }

            let Some(postscript_name) = font.postscript_name() else {
                log::warn!(
                    "font {:?} in family {:?} has no PostScript name; skipping",
                    font.full_name(),
                    name,
                );
                continue;
            };
            // Dedup is scoped to this single `load_family` call (issue #55472).
            // The same family can be reloaded later under a different `FontKey`
            // (different features/fallbacks); a global check against
            // `font_ids_by_postscript_name` would skip every already-registered
            // font and leave the second call's `font_ids` empty.
            if !postscript_names_seen.insert(postscript_name.clone()) {
                log::warn!(
                    "skipping duplicate font {:?} with PostScript name {:?} \
                     in family {:?}",
                    font.full_name(),
                    postscript_name,
                    name,
                );
                continue;
            }
            let font_id = FontId(self.fonts.len());
            font_ids.push(font_id);
            self.font_ids_by_native_font
                .insert(NativeFontKey(font.native_font(), false), font_id);
            self.postscript_names_by_font_id
                .insert(font_id, postscript_name);
            self.fonts.push(font);
        }
        Ok(font_ids)
    }

    fn advance(&self, font_id: FontId, glyph_id: GlyphId) -> Result<Size<f32>> {
        Ok(size_from_vector2f(
            self.fonts[font_id.0].advance(glyph_id.0)?,
        ))
    }

    fn glyph_for_char(&self, font_id: FontId, ch: char) -> Option<GlyphId> {
        self.fonts[font_id.0].glyph_for_char(ch).map(GlyphId)
    }

    fn id_for_native_font(&mut self, requested_font: CTFont, synthetic_bold: bool) -> FontId {
        let key = NativeFontKey(requested_font.clone(), synthetic_bold);
        if let Some(font_id) = self.font_ids_by_native_font.get(&key) {
            *font_id
        } else {
            let font_id = FontId(self.fonts.len());
            self.font_ids_by_native_font.insert(key, font_id);
            if synthetic_bold {
                self.synthetic_bold_fonts.insert(font_id);
            }
            self.postscript_names_by_font_id
                .insert(font_id, requested_font.postscript_name());
            self.fonts
                .push(unsafe { FontKitFont::from_native_font(&requested_font) });
            font_id
        }
    }

    fn is_emoji(&self, font_id: FontId) -> bool {
        self.postscript_names_by_font_id
            .get(&font_id)
            .is_some_and(|postscript_name| {
                postscript_name == "AppleColorEmoji" || postscript_name == ".AppleColorEmojiUI"
            })
    }

    fn raster_bounds(&self, params: &RenderGlyphParams) -> Result<Bounds<DevicePixels>> {
        let font = &self.fonts[params.font_id.0];
        let scale = Transform2F::from_scale(if params.is_emoji {
            1.0
        } else {
            params.scale_factor
        });
        let bounds: Bounds<DevicePixels> = bounds_from_rect_i(font.raster_bounds(
            params.glyph_id.0,
            f32::from(params.font_size)
                * if params.is_emoji {
                    params.scale_factor
                } else {
                    1.0
                },
            scale,
            HintingOptions::None,
            font_kit::canvas::RasterizationOptions::GrayscaleAa,
        )?);

        // Expand the bounds by 1 pixel on each side to give CG room for anti-aliasing.
        Ok(bounds.dilate(DevicePixels(1)))
    }

    fn rasterize_glyph(
        &self,
        params: &RenderGlyphParams,
        glyph_bounds: Bounds<DevicePixels>,
    ) -> Result<(Size<DevicePixels>, Vec<u8>)> {
        if glyph_bounds.size.width.0 == 0 || glyph_bounds.size.height.0 == 0 {
            anyhow::bail!("glyph bounds are empty");
        } else {
            // Add an extra pixel when the subpixel variant isn't zero to make room for anti-aliasing.
            let mut bitmap_size = glyph_bounds.size;
            if params.subpixel_variant.x > 0 {
                bitmap_size.width += DevicePixels(1);
            }
            if params.subpixel_variant.y > 0 {
                bitmap_size.height += DevicePixels(1);
            }
            let bitmap_size = bitmap_size;

            let mut bytes;
            let cx;
            if params.is_emoji {
                bytes = vec![0; bitmap_size.width.0 as usize * 4 * bitmap_size.height.0 as usize];
                cx = CGContext::create_bitmap_context(
                    Some(bytes.as_mut_ptr() as *mut _),
                    bitmap_size.width.0 as usize,
                    bitmap_size.height.0 as usize,
                    8,
                    bitmap_size.width.0 as usize * 4,
                    &CGColorSpace::create_device_rgb(),
                    kCGImageAlphaPremultipliedLast,
                );
            } else {
                bytes = vec![0; bitmap_size.width.0 as usize * bitmap_size.height.0 as usize];
                cx = CGContext::create_bitmap_context(
                    Some(bytes.as_mut_ptr() as *mut _),
                    bitmap_size.width.0 as usize,
                    bitmap_size.height.0 as usize,
                    8,
                    bitmap_size.width.0 as usize,
                    &CGColorSpace::create_device_gray(),
                    kCGImageAlphaOnly,
                );
            }

            // Move the origin to bottom left and account for scaling, this
            // makes drawing text consistent with the font-kit's raster_bounds.
            cx.translate(
                -glyph_bounds.origin.x.0 as CGFloat,
                (glyph_bounds.origin.y.0 + glyph_bounds.size.height.0) as CGFloat,
            );
            let context_scale = if params.is_emoji {
                1.0
            } else {
                params.scale_factor
            };
            cx.scale(context_scale as CGFloat, context_scale as CGFloat);

            let subpixel_shift = params
                .subpixel_variant
                .map(|v| v as f32 / SUBPIXEL_VARIANTS_X as f32);
            cx.set_text_drawing_mode(CGTextDrawingMode::CGTextFill);
            cx.set_allows_antialiasing(true);
            cx.set_should_antialias(true);
            cx.set_allows_font_subpixel_positioning(true);
            cx.set_should_subpixel_position_fonts(true);
            cx.set_allows_font_subpixel_quantization(false);
            cx.set_should_subpixel_quantize_fonts(false);

            cx.set_should_smooth_fonts(params.dilation > 0);
            if params.dilation > 0 {
                let luminance = params.dilation as f64 * 0.25;
                cx.set_should_smooth_fonts(true);
                cx.set_gray_fill_color(luminance, 1.0);
            } else {
                cx.set_gray_fill_color(0.0, 1.0);
            }
            if self.synthetic_bold_fonts.contains(&params.font_id) {
                // Skia's standard synthetic-bold stroke interpolation (9..36px).
                let size = f32::from(params.font_size);
                let t = ((size - 9.0) / 27.0).clamp(0.0, 1.0);
                let stroke = size * ((1.0 - t) / 24.0 + t / 32.0);
                cx.set_text_drawing_mode(CGTextDrawingMode::CGTextFillStroke);
                cx.set_line_width(stroke as CGFloat);
            }
            let native = self.fonts[params.font_id.0].native_font();
            let raster_font = if params.is_emoji {
                // CoreText selects bitmap strikes from the font's point size,
                // not the CGContext scale. Request actual device pixels so
                // Retina emoji don't enlarge a lower-resolution strike.
                emoji_font_at_size(
                    &native,
                    (f32::from(params.font_size) * params.scale_factor) as CGFloat,
                )
            } else {
                native.clone_with_font_size(f32::from(params.font_size) as CGFloat)
            };
            raster_font.draw_glyphs(
                &[params.glyph_id.0 as CGGlyph],
                &[CGPoint::new(
                    (subpixel_shift.x / context_scale) as CGFloat,
                    (subpixel_shift.y / context_scale) as CGFloat,
                )],
                cx,
            );

            if params.is_emoji {
                // Convert from RGBA with premultiplied alpha to BGRA with straight alpha.
                for pixel in bytes.chunks_exact_mut(4) {
                    swap_rgba_pa_to_bgra(pixel);
                }
            }

            Ok((bitmap_size, bytes))
        }
    }

    fn layout_line(&mut self, text: &str, font_size: Pixels, font_runs: &[FontRun]) -> LineLayout {
        // Construct the attributed string, converting UTF8 ranges to UTF16 ranges.
        let mut string = CFMutableAttributedString::new();
        let mut max_ascent = 0.0f32;
        let mut max_descent = 0.0f32;

        {
            let mut text = text;
            let mut break_ligature = true;
            for run in font_runs {
                let text_run;
                (text_run, text) = text.split_at(run.len);

                let utf16_start = string.char_len(); // insert at end of string
                // note: replace_str may silently ignore codepoints it dislikes (e.g., BOM at start of string)
                string.replace_str(&CFString::new(text_run), CFRange::init(utf16_start, 0));
                let utf16_end = string.char_len();

                let length = utf16_end - utf16_start;
                let cf_range = CFRange::init(utf16_start, length);
                let font = &self.fonts[run.font_id.0];

                let font_metrics = font.metrics();
                let font_scale = f32::from(font_size) / font_metrics.units_per_em as f32;
                // Blink rounds the scaled ascent/descent before CSS line-box
                // leading is distributed (FontMetrics::AscentDescentWithHacks).
                max_ascent = max_ascent.max((font_metrics.ascent * font_scale).round());
                max_descent = max_descent.max((-font_metrics.descent * font_scale).round());

                let actual_font_size = font_size;
                let font_size = if break_ligature {
                    px(f32::from(font_size).next_up())
                } else {
                    font_size
                };
                unsafe {
                    string.set_attribute(
                        cf_range,
                        kCTFontAttributeName,
                        &font.native_font().clone_with_font_size(font_size.into()),
                    );
                }
                // Shape fallback graphemes with explicit CSS-selected faces.
                // Otherwise CTLine can absorb Latin spaces into a CJK run and
                // choose Regular for CSS 430, unlike Chromium's fallback runs.
                // Graphemes keep combining marks and emoji ZWJ sequences intact.
                if let Some(request) = self.font_requests.get(&run.font_id).cloned() {
                    let base = font.native_font().clone_with_font_size(font_size.into());
                    let mut offset = utf16_start;
                    for grapheme in text_run.graphemes(true) {
                        let length = grapheme.encode_utf16().count() as isize;
                        if !grapheme
                            .chars()
                            .all(|ch| self.fonts[run.font_id.0].glyph_for_char(ch).is_some())
                        {
                            let text = CFString::new(grapheme);
                            unsafe extern "C" {
                                fn CTFontCreateForString(
                                    font: core_text::font::CTFontRef,
                                    string: core_foundation::string::CFStringRef,
                                    range: CFRange,
                                ) -> core_text::font::CTFontRef;
                            }
                            let fallback = unsafe {
                                CTFont::wrap_under_create_rule(CTFontCreateForString(
                                    base.as_concrete_TypeRef(),
                                    text.as_concrete_TypeRef(),
                                    CFRange::init(0, length),
                                ))
                            };
                            let family = fallback.family_name();
                            let mut fallback_request = request.clone();
                            fallback_request.family = family.clone().into();
                            fallback_request.fallbacks = None;
                            let selected = if family.contains("Emoji") {
                                // Blink explicitly requests the public emoji face.
                                // The system UI cascade uses a larger private UI
                                // face, changing both advances and bitmap size.
                                let font = core_text::font::new_from_name(
                                    "Apple Color Emoji",
                                    actual_font_size.into(),
                                )
                                .unwrap_or(fallback);
                                emoji_font_at_size(&font, actual_font_size.into())
                            } else if family == base.family_name() {
                                fallback
                            } else if let Ok(id) = self.select_font(&fallback_request) {
                                self.fonts[id.0]
                                    .native_font()
                                    .clone_with_font_size(font_size.into())
                            } else {
                                fallback
                            };
                            unsafe {
                                string.set_attribute(
                                    CFRange::init(offset, length),
                                    kCTFontAttributeName,
                                    &selected,
                                );
                            }
                        }
                        offset += length;
                    }
                }
                if let Some(request) = self.font_requests.get(&run.font_id) {
                    string.set_attribute(
                        cf_range,
                        CFString::new("GPUIRequestedWeight").as_concrete_TypeRef(),
                        &CFNumber::from(request.weight.0),
                    );
                }
                break_ligature = !break_ligature;
            }
        }
        // Retrieve the glyphs from the shaped line, converting UTF16 offsets to UTF8 offsets.
        let line = CTLine::new_with_attributed_string(string.as_concrete_TypeRef());
        let glyph_runs = line.glyph_runs();
        let mut runs = <Vec<ShapedRun>>::with_capacity(glyph_runs.len() as usize);
        let mut ix_converter = StringIndexConverter::new(text);
        for run in glyph_runs.into_iter() {
            let attributes = run.attributes().unwrap();
            let font = unsafe {
                attributes
                    .get(kCTFontAttributeName)
                    .downcast::<CTFont>()
                    .unwrap()
            };
            let requested_weight = attributes
                .find(CFString::new("GPUIRequestedWeight").as_concrete_TypeRef())
                .and_then(|v| v.downcast::<CFNumber>())
                .and_then(|n| n.to_f64())
                .unwrap_or(400.0);
            let synthetic_bold = requested_weight >= 600.0
                && (font.symbolic_traits() & core_text::font_descriptor::kCTFontBoldTrait) == 0;
            let font_id = self.id_for_native_font(font, synthetic_bold);

            let glyphs = match runs.last_mut() {
                Some(run) if run.font_id == font_id => &mut run.glyphs,
                _ => {
                    runs.push(ShapedRun {
                        font_id,
                        glyphs: Vec::with_capacity(run.glyph_count().try_into().unwrap_or(0)),
                    });
                    &mut runs.last_mut().unwrap().glyphs
                }
            };
            for ((&glyph_id, position), &glyph_utf16_ix) in run
                .glyphs()
                .iter()
                .zip(run.positions().iter())
                .zip(run.string_indices().iter())
            {
                let glyph_utf16_ix = usize::try_from(glyph_utf16_ix).unwrap();
                if ix_converter.utf16_ix > glyph_utf16_ix {
                    // We cannot reuse current index converter, as it can only seek forward. Restart the search.
                    ix_converter = StringIndexConverter::new(text);
                }
                ix_converter.advance_to_utf16_ix(glyph_utf16_ix);
                glyphs.push(ShapedGlyph {
                    id: GlyphId(glyph_id as u32),
                    position: point(position.x as f32, position.y as f32).map(px),
                    index: ix_converter.utf8_ix,
                    is_emoji: self.is_emoji(font_id),
                });
            }
        }
        let typographic_bounds = line.get_typographic_bounds();
        LineLayout {
            runs,
            font_size,
            width: typographic_bounds.width.into(),
            ascent: max_ascent.into(),
            descent: max_descent.into(),
            len: text.len(),
        }
    }
}

// Skia's exact-copy attributes suppress CoreText's extra emoji tracking and
// retain logical optical size when requesting a device-sized bitmap strike.
fn emoji_font_at_size(font: &CTFont, size: CGFloat) -> CTFont {
    let attributes = CFDictionary::from_CFType_pairs(&[
        (
            CFString::new("NSCTFontUnscaledTrackingAttribute"),
            CFNumber::from(0i32).as_CFType(),
        ),
        (
            CFString::new("NSCTFontOpticalSizeAttribute"),
            CFNumber::from(font.pt_size()).as_CFType(),
        ),
    ]);
    let descriptor = core_text::font_descriptor::new_from_attributes(&attributes);
    unsafe extern "C" {
        fn CTFontCreateCopyWithAttributes(
            font: core_text::font::CTFontRef,
            size: CGFloat,
            matrix: *const core_graphics::geometry::CGAffineTransform,
            attributes: core_text::font_descriptor::CTFontDescriptorRef,
        ) -> core_text::font::CTFontRef;
    }
    unsafe {
        CTFont::wrap_under_create_rule(CTFontCreateCopyWithAttributes(
            font.as_concrete_TypeRef(),
            size,
            std::ptr::null(),
            descriptor.as_concrete_TypeRef(),
        ))
    }
}

/// Follow Blink's MatchSystemUIFont: system UI font plus a CSS `wght` axis,
/// instead of selecting the nearest static family member. A 430-weight system
/// font still reports `.SFNS-Regular`, so it must retain its native descriptor.
/// https://chromium.googlesource.com/chromium/src/+/main/third_party/blink/renderer/platform/fonts/mac/font_matcher_mac.mm
fn chromium_system_font(weight: FontWeight, style: FontStyle) -> FontKitFont {
    use core_text::{
        font::{CTFontRef, kCTFontSystemFontType, new_ui_font_for_language},
        font_descriptor::{kCTFontBoldTrait, kCTFontItalicTrait, kCTFontVariationAttribute},
    };
    let mut font = new_ui_font_for_language(kCTFontSystemFontType, 14.0, None);
    let mut traits = 0;
    if style != FontStyle::Normal {
        traits |= kCTFontItalicTrait;
    }
    if weight.0 >= 600.0 {
        traits |= kCTFontBoldTrait;
    }
    if traits != 0 {
        font = font
            .clone_with_symbolic_traits(traits, traits)
            .unwrap_or(font);
    }
    if weight != FontWeight::NORMAL {
        let variations = CFDictionary::from_CFType_pairs(&[(
            CFNumber::from(i32::from_be_bytes(*b"wght")),
            CFNumber::from(weight.0.clamp(100.0, 900.0)),
        )]);
        let attributes = CFDictionary::from_CFType_pairs(&[(
            unsafe { CFString::wrap_under_get_rule(kCTFontVariationAttribute) },
            variations.as_CFType(),
        )]);
        let descriptor = core_text::font_descriptor::new_from_attributes(&attributes);
        unsafe extern "C" {
            fn CTFontCreateCopyWithAttributes(
                font: CTFontRef,
                size: CGFloat,
                matrix: *const core_graphics::geometry::CGAffineTransform,
                attributes: core_text::font_descriptor::CTFontDescriptorRef,
            ) -> CTFontRef;
        }
        font = unsafe {
            CTFont::wrap_under_create_rule(CTFontCreateCopyWithAttributes(
                font.as_concrete_TypeRef(),
                14.0,
                std::ptr::null(),
                descriptor.as_concrete_TypeRef(),
            ))
        };
    }
    unsafe { FontKitFont::from_native_font(&font) }
}

// Blink's ToCSSFontWeight buckets. font-kit interpolates 0.23 to 530, which
// incorrectly makes PingFang Medium ineligible for CSS's 400..=500 search.
fn css_weight_from_core_text(weight: f64) -> f32 {
    for (upper, css) in [
        (-0.70, 100.0),
        (-0.45, 200.0),
        (-0.10, 300.0),
        (0.10, 400.0),
        (0.27, 500.0),
        (0.35, 600.0),
        (0.50, 700.0),
        (0.60, 800.0),
        (1.0, 900.0),
    ] {
        if weight <= upper {
            return css;
        }
    }
    400.0
}

#[derive(Debug, Clone)]
struct StringIndexConverter<'a> {
    text: &'a str,
    /// Index in UTF-8 bytes
    utf8_ix: usize,
    /// Index in UTF-16 code units
    utf16_ix: usize,
}

impl<'a> StringIndexConverter<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            text,
            utf8_ix: 0,
            utf16_ix: 0,
        }
    }

    fn advance_to_utf16_ix(&mut self, utf16_target: usize) {
        for (ix, c) in self.text[self.utf8_ix..].char_indices() {
            if self.utf16_ix >= utf16_target {
                self.utf8_ix += ix;
                return;
            }
            self.utf16_ix += c.len_utf16();
        }
        self.utf8_ix = self.text.len();
    }
}

fn font_kit_metrics_to_metrics(metrics: Metrics) -> FontMetrics {
    FontMetrics {
        units_per_em: metrics.units_per_em,
        ascent: metrics.ascent,
        descent: metrics.descent,
        line_gap: metrics.line_gap,
        underline_position: metrics.underline_position,
        underline_thickness: metrics.underline_thickness,
        cap_height: metrics.cap_height,
        x_height: metrics.x_height,
        bounding_box: bounds_from_rect(metrics.bounding_box),
    }
}

fn bounds_from_rect(rect: RectF) -> Bounds<f32> {
    Bounds {
        origin: point(rect.origin_x(), rect.origin_y()),
        size: size(rect.width(), rect.height()),
    }
}

fn bounds_from_rect_i(rect: RectI) -> Bounds<DevicePixels> {
    Bounds {
        origin: point(DevicePixels(rect.origin_x()), DevicePixels(rect.origin_y())),
        size: size(DevicePixels(rect.width()), DevicePixels(rect.height())),
    }
}

// impl From<Vector2I> for Size<DevicePixels> {
//     fn from(value: Vector2I) -> Self {
//         size(value.x().into(), value.y().into())
//     }
// }

// impl From<RectI> for Bounds<i32> {
//     fn from(rect: RectI) -> Self {
//         Bounds {
//             origin: point(rect.origin_x(), rect.origin_y()),
//             size: size(rect.width(), rect.height()),
//         }
//     }
// }

// impl From<Point<u32>> for Vector2I {
//     fn from(size: Point<u32>) -> Self {
//         Vector2I::new(size.x as i32, size.y as i32)
//     }
// }

fn size_from_vector2f(vec: Vector2F) -> Size<f32> {
    size(vec.x(), vec.y())
}

fn fontkit_weight(value: FontWeight) -> FontkitWeight {
    FontkitWeight(value.0)
}

fn fontkit_style(style: FontStyle) -> FontkitStyle {
    match style {
        FontStyle::Normal => FontkitStyle::Normal,
        FontStyle::Italic => FontkitStyle::Italic,
        FontStyle::Oblique => FontkitStyle::Oblique,
    }
}

// Some fonts may have no attributes despite `core_text` requiring them (and panicking).
// This is the same version as `core_text` has without `expect` calls.
mod lenient_font_attributes {
    use core_foundation::{
        base::{CFRetain, CFType, TCFType},
        string::{CFString, CFStringRef},
    };
    use core_text::font_descriptor::{
        CTFontDescriptor, CTFontDescriptorCopyAttribute, kCTFontFamilyNameAttribute,
    };

    pub fn family_name(descriptor: &CTFontDescriptor) -> Option<String> {
        unsafe { get_string_attribute(descriptor, kCTFontFamilyNameAttribute) }
    }

    fn get_string_attribute(
        descriptor: &CTFontDescriptor,
        attribute: CFStringRef,
    ) -> Option<String> {
        unsafe {
            let value = CTFontDescriptorCopyAttribute(descriptor.as_concrete_TypeRef(), attribute);
            if value.is_null() {
                return None;
            }

            let value = CFType::wrap_under_create_rule(value);
            assert!(value.instance_of::<CFString>());
            let s = wrap_under_get_rule(value.as_CFTypeRef() as CFStringRef);
            Some(s.to_string())
        }
    }

    unsafe fn wrap_under_get_rule(reference: CFStringRef) -> CFString {
        unsafe {
            assert!(!reference.is_null(), "Attempted to create a NULL object.");
            let reference = CFRetain(reference as *const ::std::os::raw::c_void) as CFStringRef;
            TCFType::wrap_under_create_rule(reference)
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::MacTextSystem;
    use gpui::{FontRun, GlyphId, PlatformTextSystem, font, px};

    #[test]
    fn test_layout_line_bom_char() {
        let fonts = MacTextSystem::new();
        let font_id = fonts.font_id(&font("Helvetica")).unwrap();
        let line = "\u{feff}";
        let mut style = FontRun {
            font_id,
            len: line.len(),
        };

        let layout = fonts.layout_line(line, px(16.), &[style]);
        assert_eq!(layout.len, line.len());
        assert!(layout.runs.is_empty());

        let line = "a\u{feff}b";
        style.len = line.len();
        let layout = fonts.layout_line(line, px(16.), &[style]);
        assert_eq!(layout.len, line.len());
        assert_eq!(layout.runs.len(), 1);
        assert_eq!(layout.runs[0].glyphs.len(), 2);
        assert_eq!(layout.runs[0].glyphs[0].id, GlyphId(68u32)); // a
        // There's no glyph for \u{feff}
        assert_eq!(layout.runs[0].glyphs[1].id, GlyphId(69u32)); // b

        let line = "\u{feff}ab";
        let font_runs = &[
            FontRun {
                len: "\u{feff}".len(),
                font_id,
            },
            FontRun {
                len: "ab".len(),
                font_id,
            },
        ];
        let layout = fonts.layout_line(line, px(16.), font_runs);
        assert_eq!(layout.len, line.len());
        assert_eq!(layout.runs.len(), 1);
        assert_eq!(layout.runs[0].glyphs.len(), 2);
        // There's no glyph for \u{feff}
        assert_eq!(layout.runs[0].glyphs[0].id, GlyphId(68u32)); // a
        assert_eq!(layout.runs[0].glyphs[1].id, GlyphId(69u32)); // b
    }

    #[test]
    fn test_layout_line_zwnj_insertion() {
        let fonts = MacTextSystem::new();
        let font_id = fonts.font_id(&font("Helvetica")).unwrap();

        let text = "hello world";
        let font_runs = &[
            FontRun { font_id, len: 5 }, // "hello"
            FontRun { font_id, len: 6 }, // " world"
        ];

        let layout = fonts.layout_line(text, px(16.), font_runs);
        assert_eq!(layout.len, text.len());

        for run in &layout.runs {
            for glyph in &run.glyphs {
                assert!(
                    glyph.index < text.len(),
                    "Glyph index {} is out of bounds for text length {}",
                    glyph.index,
                    text.len()
                );
            }
        }

        // Test with different font runs - should not insert ZWNJ
        let font_id2 = fonts.font_id(&font("Times")).unwrap_or(font_id);
        let font_runs_different = &[
            FontRun { font_id, len: 5 }, // "hello"
            // " world"
            FontRun {
                font_id: font_id2,
                len: 6,
            },
        ];

        let layout2 = fonts.layout_line(text, px(16.), font_runs_different);
        assert_eq!(layout2.len, text.len());

        for run in &layout2.runs {
            for glyph in &run.glyphs {
                assert!(
                    glyph.index < text.len(),
                    "Glyph index {} is out of bounds for text length {}",
                    glyph.index,
                    text.len()
                );
            }
        }
    }

    #[test]
    fn test_layout_line_zwnj_edge_cases() {
        let fonts = MacTextSystem::new();
        let font_id = fonts.font_id(&font("Helvetica")).unwrap();

        let text = "hello";
        let font_runs = &[FontRun { font_id, len: 5 }];
        let layout = fonts.layout_line(text, px(16.), font_runs);
        assert_eq!(layout.len, text.len());

        let text = "abc";
        let font_runs = &[
            FontRun { font_id, len: 1 }, // "a"
            FontRun { font_id, len: 1 }, // "b"
            FontRun { font_id, len: 1 }, // "c"
        ];
        let layout = fonts.layout_line(text, px(16.), font_runs);
        assert_eq!(layout.len, text.len());

        for run in &layout.runs {
            for glyph in &run.glyphs {
                assert!(
                    glyph.index < text.len(),
                    "Glyph index {} is out of bounds for text length {}",
                    glyph.index,
                    text.len()
                );
            }
        }

        // Test with empty text
        let text = "";
        let font_runs = &[];
        let layout = fonts.layout_line(text, px(16.), font_runs);
        assert_eq!(layout.len, 0);
        assert!(layout.runs.is_empty());
    }
}

#[cfg(test)]
mod typography_tests {
    use super::*;

    fn shape(system: &MacTextSystem, text: &str, weight: f32) -> LineLayout {
        let mut font = gpui::font(".SystemUIFont");
        font.weight = FontWeight(weight);
        font.fallbacks = Some(FontFallbacks::from_fonts(vec!["PingFang SC".into()]));
        let font_id = system.font_id(&font).unwrap();
        system.layout_line(
            text,
            px(14.0),
            &[FontRun {
                len: text.len(),
                font_id,
            }],
        )
    }

    #[test]
    fn typography_variable_weights_keep_distinct_native_instances() {
        let system = MacTextSystem::new();
        let widths = [400.0, 430.0, 500.0, 600.0]
            .map(|weight| shape(&system, "Hamburgefontsiv GPUI 0123456789", weight).width);
        assert!(
            widths.windows(2).all(|pair| pair[0] < pair[1]),
            "{widths:?}"
        );
        // Shaping 400 again after 430/500 must not reuse a same-name instance.
        assert_eq!(
            shape(&system, "Hamburgefontsiv GPUI 0123456789", 400.0).width,
            widths[0]
        );
    }

    #[test]
    fn typography_cjk_fallback_preserves_weight_and_latin_spaces() {
        let system = MacTextSystem::new();
        for (weight, expected) in [
            (400.0, "PingFangSC-Regular"),
            (430.0, "PingFangSC-Medium"),
            (500.0, "PingFangSC-Medium"),
            (600.0, "PingFangSC-Semibold"),
        ] {
            let line = shape(&system, "新对话 字体渲染 设置与项目", weight);
            let state = system.0.read();
            let names: Vec<_> = line
                .runs
                .iter()
                .map(|run| state.fonts[run.font_id.0].postscript_name().unwrap())
                .collect();
            assert!(
                names.iter().any(|name| name == expected),
                "{weight}: {names:?}"
            );
            assert!(
                names.iter().any(|name| name.starts_with(".SFNS")),
                "spaces must keep the primary font: {names:?}"
            );
        }
    }

    #[test]
    fn typography_repeated_layout_keeps_cache_bounded_and_utf8_indices() {
        let system = MacTextSystem::new();
        let text = "中文 e\u{301} 👩‍💻 😀 ffi";
        let line = shape(&system, text, 430.0);
        let fonts = system.0.read().fonts.len();
        for _ in 0..32 {
            let repeated = shape(&system, text, 430.0);
            assert_eq!(repeated.width, line.width);
            for run in repeated.runs {
                for glyph in run.glyphs {
                    assert!(text.is_char_boundary(glyph.index));
                }
            }
        }
        assert_eq!(system.0.read().fonts.len(), fonts);
    }

    #[test]
    fn typography_emoji_uses_untracked_public_face() {
        let system = MacTextSystem::new();
        let line = shape(&system, "Emoji 😀 中文 Hello 🌍", 430.0);
        assert!((f32::from(line.width) - 147.5).abs() < 0.02);
        let state = system.0.read();
        let emojis = line
            .runs
            .iter()
            .filter(|run| state.is_emoji(run.font_id))
            .collect::<Vec<_>>();
        assert_eq!(emojis.len(), 2);
        for run in emojis {
            assert_eq!(
                state.fonts[run.font_id.0].postscript_name().as_deref(),
                Some("AppleColorEmoji")
            );
        }
    }

    #[test]
    fn typography_synthetic_bold_does_not_poison_regular_face() {
        let system = MacTextSystem::new();
        let regular = gpui::font("Zapfino");
        let mut bold = regular.clone();
        bold.weight = FontWeight::BOLD;
        let regular_id = system.font_id(&regular).unwrap();
        let bold_id = system.font_id(&bold).unwrap();
        assert_ne!(regular_id, bold_id);
        let bold_line = system.layout_line(
            "Test",
            px(17.0),
            &[FontRun {
                len: 4,
                font_id: bold_id,
            }],
        );
        let regular_line = system.layout_line(
            "Test",
            px(17.0),
            &[FontRun {
                len: 4,
                font_id: regular_id,
            }],
        );
        let state = system.0.read();
        assert!(
            bold_line
                .runs
                .iter()
                .all(|run| state.synthetic_bold_fonts.contains(&run.font_id))
        );
        assert!(
            regular_line
                .runs
                .iter()
                .all(|run| !state.synthetic_bold_fonts.contains(&run.font_id))
        );
    }
}
