use ambassador::delegatable_trait;
use wm_common::TilingDirection;
use wm_platform::Rect;

const ACCORDION_PADDING: i32 = 30;

/// Rect of a child inside an accordion container, following `AeroSpace`'s
/// accordion layout: children overlap and peek out by a fixed padding.
#[must_use]
pub fn accordion_child_rect(
  parent_rect: &Rect,
  tiling_direction: &TilingDirection,
  child_count: usize,
  child_index: usize,
  focused_index: usize,
) -> Rect {
  let last_index = child_count.saturating_sub(1);

  let (leading_padding, trailing_padding) = match child_index {
    0 if child_count == 1 => (0, 0),
    0 => (0, ACCORDION_PADDING),
    index if index == last_index => (ACCORDION_PADDING, 0),
    index if index + 1 == focused_index => (0, 2 * ACCORDION_PADDING),
    index if index == focused_index + 1 => (2 * ACCORDION_PADDING, 0),
    _ => (ACCORDION_PADDING, ACCORDION_PADDING),
  };

  match tiling_direction {
    TilingDirection::Horizontal => Rect::from_xy(
      parent_rect.x() + leading_padding,
      parent_rect.y(),
      parent_rect.width() - leading_padding - trailing_padding,
      parent_rect.height(),
    ),
    TilingDirection::Vertical => Rect::from_xy(
      parent_rect.x(),
      parent_rect.y() + leading_padding,
      parent_rect.width(),
      parent_rect.height() - leading_padding - trailing_padding,
    ),
  }
}

#[delegatable_trait]
pub trait PositionGetters {
  fn to_rect(&self) -> anyhow::Result<Rect>;
}

/// Implements the `PositionGetters` trait for tiling containers that can
/// be resized. This is used by `SplitContainer` and `TilingWindow`.
///
/// Expects that the struct has a wrapping `RefCell` containing a struct
/// with an `id` and a `parent` field.
#[macro_export]
macro_rules! impl_position_getters_as_resizable {
  ($struct_name:ident) => {
    impl PositionGetters for $struct_name {
      fn to_rect(&self) -> anyhow::Result<Rect> {
        let parent = self
          .parent()
          .and_then(|parent| parent.as_direction_container().ok())
          .context("Parent does not have a tiling direction.")?;

        let parent_rect = parent.to_rect()?;

        if parent.is_accordion() {
          return Ok($crate::traits::accordion_child_rect(
            &parent_rect,
            &parent.tiling_direction(),
            self.tiling_siblings().count() + 1,
            self.prev_siblings()
              .filter(|sibling| sibling.as_tiling_container().is_ok())
              .count(),
            parent
              .child_focus_order()
              .find_map(|child| child.as_tiling_container().ok())
              .and_then(|focused| {
                parent
                  .tiling_children()
                  .position(|child| child.id() == focused.id())
              })
              .unwrap_or(0),
          ));
        }

        let (horizontal_gap, vertical_gap) = self.inner_gaps()?;
        let inner_gap = match parent.tiling_direction() {
          TilingDirection::Vertical => vertical_gap,
          TilingDirection::Horizontal => horizontal_gap,
        };

        #[allow(
          clippy::cast_precision_loss,
          clippy::cast_possible_truncation,
          clippy::cast_possible_wrap
        )]
        let (width, height) = match parent.tiling_direction() {
          TilingDirection::Vertical => {
            let available_height = parent_rect.height()
              - inner_gap * self.tiling_siblings().count() as i32;

            let height =
              (self.tiling_size() * available_height as f32) as i32;

            (parent_rect.width(), height)
          }
          TilingDirection::Horizontal => {
            let available_width = parent_rect.width()
              - inner_gap * self.tiling_siblings().count() as i32;

            let width =
              (available_width as f32 * self.tiling_size()).round() as i32;

            (width, parent_rect.height())
          }
        };

        let (x, y) = {
          let mut prev_siblings = self
            .prev_siblings()
            .filter_map(|sibling| sibling.as_tiling_container().ok());

          match prev_siblings.next() {
            None => (parent_rect.x(), parent_rect.y()),
            Some(sibling) => {
              let sibling_rect = sibling.to_rect()?;

              match parent.tiling_direction() {
                TilingDirection::Vertical => (
                  parent_rect.x(),
                  sibling_rect.y() + sibling_rect.height() + inner_gap,
                ),
                TilingDirection::Horizontal => (
                  sibling_rect.x() + sibling_rect.width() + inner_gap,
                  parent_rect.y(),
                ),
              }
            }
          }
        };

        Ok(Rect::from_xy(x, y, width, height))
      }
    }
  };
}
