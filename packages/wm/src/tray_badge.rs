use image::{imageops, Rgba, RgbaImage};

const ICON_SIZE: u32 = 64;
const BADGE_CENTER: f32 = 37.0;
const BADGE_RADIUS: f32 = 27.0;
const BADGE_OUTLINE_WIDTH: f32 = 3.0;
const BADGE_FILL: Rgba<u8> = Rgba([232, 89, 12, 255]);
const BADGE_OUTLINE: Rgba<u8> = Rgba([24, 24, 27, 255]);
const LABEL_COLOR: Rgba<u8> = Rgba([255, 255, 255, 255]);
const GLYPH_WIDTH: u32 = 5;
const GLYPH_HEIGHT: u32 = 7;
const GLYPH_SPACING: u32 = 1;
const MAX_LABEL_CHARS: usize = 2;

/// Renders the tray icon with a badge showing the given workspace label.
///
/// Only digits are drawn. Returns the plain icon when the label has no
/// drawable characters.
pub fn render_badged_icon(base_icon: &RgbaImage, label: &str) -> RgbaImage {
  let mut icon = imageops::resize(
    base_icon,
    ICON_SIZE,
    ICON_SIZE,
    imageops::FilterType::Triangle,
  );

  let glyphs = label
    .chars()
    .filter_map(digit_glyph)
    .take(MAX_LABEL_CHARS)
    .collect::<Vec<_>>();

  if glyphs.is_empty() {
    return icon;
  }

  draw_badge_circle(&mut icon);
  draw_glyphs(&mut icon, &glyphs);
  icon
}

fn draw_badge_circle(icon: &mut RgbaImage) {
  for (x, y, pixel) in icon.enumerate_pixels_mut() {
    #[allow(clippy::cast_precision_loss)]
    let distance = ((x as f32 + 0.5 - BADGE_CENTER).powi(2)
      + (y as f32 + 0.5 - BADGE_CENTER).powi(2))
    .sqrt();

    let outline_coverage = (BADGE_RADIUS + 0.5 - distance).clamp(0.0, 1.0);
    let fill_coverage =
      (BADGE_RADIUS - BADGE_OUTLINE_WIDTH + 0.5 - distance).clamp(0.0, 1.0);

    blend(pixel, BADGE_OUTLINE, outline_coverage);
    blend(pixel, BADGE_FILL, fill_coverage);
  }
}

fn draw_glyphs(icon: &mut RgbaImage, glyphs: &[[u8; 7]]) {
  let scale: u32 = if glyphs.len() == 1 { 6 } else { 4 };

  #[allow(clippy::cast_possible_truncation)]
  let glyph_count = glyphs.len() as u32;
  let label_width = glyph_count * GLYPH_WIDTH * scale
    + (glyph_count - 1) * GLYPH_SPACING * scale;
  let label_height = GLYPH_HEIGHT * scale;

  #[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
  )]
  let (origin_x, origin_y) = (
    (BADGE_CENTER - label_width as f32 / 2.0).round() as u32,
    (BADGE_CENTER - label_height as f32 / 2.0).round() as u32,
  );

  for (glyph_index, glyph) in (0u32..).zip(glyphs) {
    let glyph_x =
      origin_x + glyph_index * (GLYPH_WIDTH + GLYPH_SPACING) * scale;

    for (row, row_bits) in (0u32..).zip(glyph) {
      for column in 0..GLYPH_WIDTH {
        let is_lit = row_bits & (1 << (GLYPH_WIDTH - 1 - column)) != 0;
        if !is_lit {
          continue;
        }

        for dy in 0..scale {
          for dx in 0..scale {
            let x = glyph_x + column * scale + dx;
            let y = origin_y + row * scale + dy;
            if x < ICON_SIZE && y < ICON_SIZE {
              icon.put_pixel(x, y, LABEL_COLOR);
            }
          }
        }
      }
    }
  }
}

fn blend(pixel: &mut Rgba<u8>, color: Rgba<u8>, coverage: f32) {
  if coverage <= 0.0 {
    return;
  }

  for channel in 0..4 {
    let current = f32::from(pixel.0[channel]);
    let target = f32::from(color.0[channel]);

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    {
      pixel.0[channel] =
        (current + (target - current) * coverage).round() as u8;
    }
  }
}

/// 5x7 bitmap rows for a digit, most significant bit on the left.
fn digit_glyph(character: char) -> Option<[u8; 7]> {
  let glyph = match character {
    '0' => [0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E],
    '1' => [0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E],
    '2' => [0x0E, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1F],
    '3' => [0x1F, 0x02, 0x04, 0x02, 0x01, 0x11, 0x0E],
    '4' => [0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02],
    '5' => [0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E],
    '6' => [0x06, 0x08, 0x10, 0x1E, 0x11, 0x11, 0x0E],
    '7' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
    '8' => [0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E],
    '9' => [0x0E, 0x11, 0x11, 0x0F, 0x01, 0x02, 0x0C],
    _ => return None,
  };

  Some(glyph)
}
