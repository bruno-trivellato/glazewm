use image::{imageops, Rgba, RgbaImage};

const ICON_SIZE: u32 = 64;
const BADGE_CENTER: f32 = 37.0;
const BADGE_RADIUS: f32 = 27.0;
const BADGE_OUTLINE_WIDTH: f32 = 3.0;
const BADGE_FILL_COLOR: Rgba<u8> = Rgba([232, 89, 12, 255]);
const BADGE_OUTLINE_COLOR: Rgba<u8> = Rgba([24, 24, 27, 255]);
const DIGIT_COLOR: Rgba<u8> = Rgba([255, 255, 255, 255]);
const MAX_DIGITS_IN_BADGE: usize = 2;
const PIXEL_SCALE_FOR_ONE_DIGIT: u32 = 6;
const PIXEL_SCALE_FOR_TWO_DIGITS: u32 = 4;
const GAP_BETWEEN_DIGITS_IN_GLYPH_PIXELS: u32 = 1;

const LIT_GLYPH_PIXEL: char = '#';
const GLYPH_WIDTH: u32 = 5;
const GLYPH_HEIGHT: u32 = 7;

type DigitGlyph = [&'static str; GLYPH_HEIGHT as usize];

pub fn render_icon_with_workspace_badge(
  icon_without_badge: &RgbaImage,
  workspace_name: &str,
) -> RgbaImage {
  let mut icon = imageops::resize(
    icon_without_badge,
    ICON_SIZE,
    ICON_SIZE,
    imageops::FilterType::Triangle,
  );

  let digit_glyphs = workspace_name
    .chars()
    .filter_map(digit_glyph)
    .take(MAX_DIGITS_IN_BADGE)
    .collect::<Vec<_>>();

  let workspace_name_has_no_digits = digit_glyphs.is_empty();
  if workspace_name_has_no_digits {
    return icon;
  }

  draw_badge_circle(&mut icon);
  draw_digits_centered_in_badge(&mut icon, &digit_glyphs);
  icon
}

fn draw_badge_circle(icon: &mut RgbaImage) {
  let fill_radius = BADGE_RADIUS - BADGE_OUTLINE_WIDTH;

  for (x, y, pixel) in icon.enumerate_pixels_mut() {
    let distance_from_center = distance_from_badge_center(x, y);

    blend(
      pixel,
      BADGE_OUTLINE_COLOR,
      antialiased_disc_coverage(distance_from_center, BADGE_RADIUS),
    );
    blend(
      pixel,
      BADGE_FILL_COLOR,
      antialiased_disc_coverage(distance_from_center, fill_radius),
    );
  }
}

#[allow(clippy::cast_precision_loss)]
fn distance_from_badge_center(x: u32, y: u32) -> f32 {
  let pixel_center_x = x as f32 + 0.5;
  let pixel_center_y = y as f32 + 0.5;

  ((pixel_center_x - BADGE_CENTER).powi(2)
    + (pixel_center_y - BADGE_CENTER).powi(2))
  .sqrt()
}

fn antialiased_disc_coverage(distance_from_center: f32, radius: f32) -> f32 {
  (radius + 0.5 - distance_from_center).clamp(0.0, 1.0)
}

fn draw_digits_centered_in_badge(icon: &mut RgbaImage, glyphs: &[DigitGlyph]) {
  let pixel_scale = if glyphs.len() == 1 {
    PIXEL_SCALE_FOR_ONE_DIGIT
  } else {
    PIXEL_SCALE_FOR_TWO_DIGITS
  };

  #[allow(clippy::cast_possible_truncation)]
  let digit_count = glyphs.len() as u32;
  let digit_advance = (GLYPH_WIDTH + GAP_BETWEEN_DIGITS_IN_GLYPH_PIXELS) * pixel_scale;
  let text_width = digit_count * digit_advance
    - GAP_BETWEEN_DIGITS_IN_GLYPH_PIXELS * pixel_scale;
  let text_height = GLYPH_HEIGHT * pixel_scale;

  let text_left = centered_start(text_width);
  let text_top = centered_start(text_height);

  for (digit_index, glyph) in (0u32..).zip(glyphs) {
    let digit_left = text_left + digit_index * digit_advance;
    draw_glyph(icon, glyph, digit_left, text_top, pixel_scale);
  }
}

#[allow(
  clippy::cast_possible_truncation,
  clippy::cast_sign_loss,
  clippy::cast_precision_loss
)]
fn centered_start(length: u32) -> u32 {
  (BADGE_CENTER - length as f32 / 2.0).round() as u32
}

fn draw_glyph(
  icon: &mut RgbaImage,
  glyph: &DigitGlyph,
  left: u32,
  top: u32,
  pixel_scale: u32,
) {
  for (glyph_row, row_pixels) in (0u32..).zip(glyph) {
    for (glyph_column, glyph_pixel) in (0u32..).zip(row_pixels.chars()) {
      if glyph_pixel != LIT_GLYPH_PIXEL {
        continue;
      }

      fill_square(
        icon,
        left + glyph_column * pixel_scale,
        top + glyph_row * pixel_scale,
        pixel_scale,
      );
    }
  }
}

fn fill_square(icon: &mut RgbaImage, left: u32, top: u32, size: u32) {
  for y in top..top + size {
    for x in left..left + size {
      if x < ICON_SIZE && y < ICON_SIZE {
        icon.put_pixel(x, y, DIGIT_COLOR);
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

fn digit_glyph(character: char) -> Option<DigitGlyph> {
  let glyph = match character {
    '0' => [".###.", "#...#", "#..##", "#.#.#", "##..#", "#...#", ".###."],
    '1' => ["..#..", ".##..", "..#..", "..#..", "..#..", "..#..", ".###."],
    '2' => [".###.", "#...#", "....#", "...#.", "..#..", ".#...", "#####"],
    '3' => ["#####", "...#.", "..#..", "...#.", "....#", "#...#", ".###."],
    '4' => ["...#.", "..##.", ".#.#.", "#..#.", "#####", "...#.", "...#."],
    '5' => ["#####", "#....", "####.", "....#", "....#", "#...#", ".###."],
    '6' => ["..##.", ".#...", "#....", "####.", "#...#", "#...#", ".###."],
    '7' => ["#####", "....#", "...#.", "..#..", ".#...", ".#...", ".#..."],
    '8' => [".###.", "#...#", "#...#", ".###.", "#...#", "#...#", ".###."],
    '9' => [".###.", "#...#", "#...#", ".####", "....#", "...#.", ".##.."],
    _ => return None,
  };

  Some(glyph)
}
