use std::{
  collections::HashSet,
  fs,
  path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use wm_common::{
  ContainerDto, InvokeCommand, TilingDirection, WindowDto, WorkspaceDto,
};

use crate::{
  traits::{CommonGetters, TilingDirectionGetters},
  user_config::UserConfig,
  wm::WindowManager,
  wm_command_builders::{focus_workspace, move_to_workspace},
};

const ARRANGEMENT_FILE_NAME: &str = "session.json";

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowArrangementSnapshot {
  focused_workspace: Option<String>,
  workspaces: Vec<WorkspaceSnapshot>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceSnapshot {
  name: String,
  tiling_direction: TilingDirection,
  is_accordion: bool,
  #[serde(rename = "windows")]
  windows_in_tree_order: Vec<WindowSnapshot>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct WindowSnapshot {
  handle: isize,
  process_name: String,
  class_name: String,
  title: String,
}

pub struct WindowArrangementFile {
  file_path: PathBuf,
  last_written_snapshot: Option<WindowArrangementSnapshot>,
}

impl WindowArrangementFile {
  pub fn next_to_config(config_path: &Path) -> Self {
    let config_directory =
      config_path.parent().unwrap_or_else(|| Path::new("."));

    Self {
      file_path: config_directory.join(ARRANGEMENT_FILE_NAME),
      last_written_snapshot: None,
    }
  }

  pub fn read_saved_arrangement(&self) -> Option<WindowArrangementSnapshot> {
    let contents = fs::read_to_string(&self.file_path).ok()?;

    serde_json::from_str(&contents)
      .inspect_err(|err| {
        tracing::warn!("Ignoring unreadable window arrangement: {err}");
      })
      .ok()
  }

  pub fn save_if_changed(
    &mut self,
    workspaces: &[WorkspaceDto],
  ) -> anyhow::Result<()> {
    let snapshot = WindowArrangementSnapshot::of(workspaces);

    let has_nothing_to_save = snapshot.workspaces.is_empty();
    let is_already_saved =
      self.last_written_snapshot.as_ref() == Some(&snapshot);

    if has_nothing_to_save || is_already_saved {
      return Ok(());
    }

    let partially_written_path = self.file_path.with_extension("json.tmp");
    fs::write(
      &partially_written_path,
      serde_json::to_string_pretty(&snapshot)?,
    )?;
    fs::rename(&partially_written_path, &self.file_path)?;

    self.last_written_snapshot = Some(snapshot);
    Ok(())
  }
}

impl WindowArrangementSnapshot {
  fn of(workspaces: &[WorkspaceDto]) -> Self {
    let focused_workspace = workspaces
      .iter()
      .find(|workspace| workspace.has_focus)
      .map(|workspace| workspace.name.clone());

    Self {
      focused_workspace,
      workspaces: workspaces.iter().map(WorkspaceSnapshot::of).collect(),
    }
  }
}

impl WorkspaceSnapshot {
  fn of(workspace: &WorkspaceDto) -> Self {
    let mut windows_in_tree_order = Vec::new();
    collect_windows_in_tree_order(
      &workspace.children,
      &mut windows_in_tree_order,
    );

    Self {
      name: workspace.name.clone(),
      tiling_direction: workspace.tiling_direction.clone(),
      is_accordion: workspace.is_accordion,
      windows_in_tree_order: windows_in_tree_order
        .into_iter()
        .map(WindowSnapshot::of)
        .collect(),
    }
  }
}

impl WindowSnapshot {
  fn of(window: &WindowDto) -> Self {
    Self {
      handle: window.handle,
      process_name: window.process_name.clone(),
      class_name: window.class_name.clone(),
      title: window.title.clone(),
    }
  }

  fn has_same_identity_as(&self, window: &WindowDto) -> bool {
    window.process_name == self.process_name
      && window.class_name == self.class_name
      && window.title == self.title
  }
}

fn collect_windows_in_tree_order<'a>(
  children: &'a [ContainerDto],
  windows: &mut Vec<&'a WindowDto>,
) {
  for child in children {
    match child {
      ContainerDto::Window(window) => windows.push(window),
      ContainerDto::Split(split) => {
        collect_windows_in_tree_order(&split.children, windows);
      }
      ContainerDto::Root(_)
      | ContainerDto::Monitor(_)
      | ContainerDto::Workspace(_) => {}
    }
  }
}

pub fn restore_window_arrangement(
  snapshot: &WindowArrangementSnapshot,
  wm: &mut WindowManager,
  config: &mut UserConfig,
) -> anyhow::Result<()> {
  let restorable_workspaces = snapshot
    .workspaces
    .iter()
    .filter(|workspace| is_configured_workspace(&workspace.name, config))
    .collect::<Vec<_>>();

  let restored_window_count = move_windows_back_to_saved_workspaces(
    &restorable_workspaces,
    wm,
    config,
  )?;

  for saved_workspace in &restorable_workspaces {
    reapply_saved_workspace_layout(saved_workspace, wm, config);
  }

  if let Some(focused_workspace) = &snapshot.focused_workspace {
    if is_configured_workspace(focused_workspace, config) {
      wm.process_commands(
        &vec![focus_workspace(focused_workspace)],
        None,
        config,
      )?;
    }
  }

  tracing::info!(
    "Restored {restored_window_count} windows from the saved arrangement."
  );

  Ok(())
}

fn is_configured_workspace(name: &str, config: &UserConfig) -> bool {
  config
    .value
    .workspaces
    .iter()
    .any(|workspace| workspace.name == name)
}

fn move_windows_back_to_saved_workspaces(
  saved_workspaces: &[&WorkspaceSnapshot],
  wm: &mut WindowManager,
  config: &mut UserConfig,
) -> anyhow::Result<usize> {
  let open_windows = open_window_dtos(wm)?;
  let mut already_placed_window_ids = HashSet::new();

  for saved_workspace in saved_workspaces {
    for saved_window in &saved_workspace.windows_in_tree_order {
      let unplaced_open_windows = open_windows
        .iter()
        .filter(|window| !already_placed_window_ids.contains(&window.id))
        .collect::<Vec<_>>();

      let matching_window_id =
        find_window_still_open_with_same_handle(
          saved_window,
          &unplaced_open_windows,
        )
        .or_else(|| {
          find_window_reopened_with_same_identity(
            saved_window,
            &unplaced_open_windows,
          )
        });

      let Some(window_id) = matching_window_id else {
        continue;
      };

      already_placed_window_ids.insert(window_id);

      if let Err(err) = wm.process_commands(
        &vec![move_to_workspace(&saved_workspace.name)],
        Some(window_id),
        config,
      ) {
        tracing::warn!("Failed to move window back to its workspace: {err}");
      }
    }
  }

  Ok(already_placed_window_ids.len())
}

fn open_window_dtos(wm: &WindowManager) -> anyhow::Result<Vec<WindowDto>> {
  let mut window_dtos = Vec::new();

  for window in wm.state.windows() {
    if let ContainerDto::Window(window_dto) = window.to_dto()? {
      window_dtos.push(window_dto);
    }
  }

  Ok(window_dtos)
}

fn find_window_still_open_with_same_handle(
  saved_window: &WindowSnapshot,
  candidates: &[&WindowDto],
) -> Option<Uuid> {
  candidates
    .iter()
    .find(|window| window.handle == saved_window.handle)
    .map(|window| window.id)
}

fn find_window_reopened_with_same_identity(
  saved_window: &WindowSnapshot,
  candidates: &[&WindowDto],
) -> Option<Uuid> {
  candidates
    .iter()
    .find(|window| saved_window.has_same_identity_as(window))
    .map(|window| window.id)
}

fn reapply_saved_workspace_layout(
  saved_workspace: &WorkspaceSnapshot,
  wm: &mut WindowManager,
  config: &mut UserConfig,
) {
  let Some(workspace) = wm
    .state
    .workspaces()
    .into_iter()
    .find(|workspace| workspace.config().name == saved_workspace.name)
  else {
    return;
  };

  let mut layout_commands = Vec::new();

  if workspace.tiling_direction() != saved_workspace.tiling_direction {
    layout_commands.push(InvokeCommand::SetTilingDirection {
      tiling_direction: saved_workspace.tiling_direction.clone(),
    });
  }

  if workspace.is_accordion() != saved_workspace.is_accordion {
    layout_commands.push(InvokeCommand::ToggleAccordion);
  }

  if layout_commands.is_empty() {
    return;
  }

  if let Err(err) =
    wm.process_commands(&layout_commands, Some(workspace.id()), config)
  {
    tracing::warn!("Failed to reapply workspace layout: {err}");
  }
}
