use anyhow::Context;

use crate::{
  models::{Container, DirectionContainer},
  traits::{CommonGetters, TilingDirectionGetters},
  wm_state::WmState,
};

/// Toggles the accordion layout of the given workspace, or of the
/// direction container holding the given tiling window.
pub fn toggle_accordion(
  container: Container,
  state: &mut WmState,
) -> anyhow::Result<()> {
  let workspace = container.workspace().context("No workspace.")?;

  let direction_container: DirectionContainer = match container {
    Container::Workspace(workspace) => workspace.into(),
    Container::TilingWindow(tiling_window) => {
      match tiling_window.direction_container() {
        Some(direction_container) => direction_container,
        None => return Ok(()),
      }
    }
    _ => return Ok(()),
  };

  direction_container.set_accordion(!direction_container.is_accordion());

  state
    .pending_sync
    .queue_container_to_redraw(direction_container)
    .queue_workspace_to_reorder(workspace);

  Ok(())
}
