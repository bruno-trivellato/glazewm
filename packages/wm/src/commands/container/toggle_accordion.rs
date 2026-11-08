use anyhow::Context;

use crate::{
  models::{Container, DirectionContainer},
  traits::{CommonGetters, TilingDirectionGetters},
  wm_state::WmState,
};

pub fn toggle_accordion(
  container: Container,
  state: &mut WmState,
) -> anyhow::Result<()> {
  let workspace = container.workspace().context("No workspace.")?;

  let Some(container_to_toggle) = accordion_candidate_for(container) else {
    return Ok(());
  };

  container_to_toggle.set_accordion(!container_to_toggle.is_accordion());

  state
    .pending_sync
    .queue_container_to_redraw(container_to_toggle)
    .queue_workspace_to_reorder(workspace);

  Ok(())
}

fn accordion_candidate_for(container: Container) -> Option<DirectionContainer> {
  match container {
    Container::Workspace(workspace) => Some(workspace.into()),
    Container::TilingWindow(tiling_window) => {
      tiling_window.direction_container()
    }
    _ => None,
  }
}
