use ambassador::delegatable_trait;
use wm_common::TilingDirection;
use wm_platform::Rect;

use crate::{models::DirectionContainer, traits::CommonGetters};

const ACCORDION_PEEK_SIZE: i32 = 30;

pub struct AccordionSlot {
  pub child_count: usize,
  pub child_index: usize,
  pub focused_child_index: usize,
}

struct AccordionInsets {
  leading: i32,
  trailing: i32,
}

impl AccordionSlot {
  fn insets(&self) -> AccordionInsets {
    let is_only_child = self.child_count == 1;
    let is_first_child = self.child_index == 0;
    let is_last_child = self.child_index + 1 == self.child_count;
    let is_just_before_focused =
      self.child_index + 1 == self.focused_child_index;
    let is_just_after_focused =
      self.child_index == self.focused_child_index + 1;

    let (leading, trailing) = if is_only_child {
      (0, 0)
    } else if is_first_child {
      (0, ACCORDION_PEEK_SIZE)
    } else if is_last_child {
      (ACCORDION_PEEK_SIZE, 0)
    } else if is_just_before_focused {
      (0, 2 * ACCORDION_PEEK_SIZE)
    } else if is_just_after_focused {
      (2 * ACCORDION_PEEK_SIZE, 0)
    } else {
      (ACCORDION_PEEK_SIZE, ACCORDION_PEEK_SIZE)
    };

    AccordionInsets { leading, trailing }
  }
}

#[must_use]
pub fn accordion_child_rect(
  parent_rect: &Rect,
  tiling_direction: &TilingDirection,
  slot: &AccordionSlot,
) -> Rect {
  let insets = slot.insets();

  match tiling_direction {
    TilingDirection::Horizontal => Rect::from_xy(
      parent_rect.x() + insets.leading,
      parent_rect.y(),
      parent_rect.width() - insets.leading - insets.trailing,
      parent_rect.height(),
    ),
    TilingDirection::Vertical => Rect::from_xy(
      parent_rect.x(),
      parent_rect.y() + insets.leading,
      parent_rect.width(),
      parent_rect.height() - insets.leading - insets.trailing,
    ),
  }
}

#[must_use]
pub fn last_focused_tiling_child_index(parent: &DirectionContainer) -> usize {
  let last_focused_tiling_child = parent
    .child_focus_order()
    .find_map(|child| child.as_tiling_container().ok());

  last_focused_tiling_child
    .and_then(|focused_child| {
      parent
        .tiling_children()
        .position(|child| child.id() == focused_child.id())
    })
    .unwrap_or(0)
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
          let slot = $crate::traits::AccordionSlot {
            child_count: self.tiling_siblings().count() + 1,
            child_index: self
              .prev_siblings()
              .filter(|sibling| sibling.as_tiling_container().is_ok())
              .count(),
            focused_child_index:
              $crate::traits::last_focused_tiling_child_index(&parent),
          };

          return Ok($crate::traits::accordion_child_rect(
            &parent_rect,
            &parent.tiling_direction(),
            &slot,
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

#[cfg(test)]
mod tests {
  use wm_common::TilingDirection;
  use wm_platform::Rect;

  use super::{accordion_child_rect, AccordionSlot};

  fn horizontal_accordion_child_span(
    child_count: usize,
    child_index: usize,
    focused_child_index: usize,
  ) -> (i32, i32) {
    let rect = accordion_child_rect(
      &Rect::from_xy(0, 0, 1000, 500),
      &TilingDirection::Horizontal,
      &AccordionSlot {
        child_count,
        child_index,
        focused_child_index,
      },
    );

    (rect.x(), rect.width())
  }

  #[test]
  fn accordion_children_peek_out_around_the_focused_child() {
    assert_eq!(horizontal_accordion_child_span(1, 0, 0), (0, 1000));

    assert_eq!(horizontal_accordion_child_span(4, 0, 1), (0, 970));
    assert_eq!(horizontal_accordion_child_span(4, 1, 1), (30, 940));
    assert_eq!(horizontal_accordion_child_span(4, 2, 1), (60, 940));
    assert_eq!(horizontal_accordion_child_span(4, 3, 1), (30, 970));
  }
}
