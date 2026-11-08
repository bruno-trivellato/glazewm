use std::{
  collections::HashSet,
  fs,
  path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use wm_common::{
  ContainerDto, InvokeCommand, InvokeFocusCommand, InvokeMoveCommand,
  TilingDirection, WindowDto, WorkspaceDto,
};

use crate::{
  traits::{CommonGetters, TilingDirectionGetters},
  user_config::UserConfig,
  wm::WindowManager,
};

const SESSION_FILE_NAME: &str = "session.json";

/// Snapshot of which windows live in which workspace, and each
/// workspace's layout.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
  focused_workspace: Option<String>,
  workspaces: Vec<SessionWorkspace>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionWorkspace {
  name: String,
  tiling_direction: TilingDirection,
  is_accordion: bool,
  windows: Vec<SessionWindow>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionWindow {
  handle: isize,
  process_name: String,
  class_name: String,
  title: String,
}

/// Persists the session next to the user config, skipping unchanged
/// snapshots.
pub struct SessionStore {
  path: PathBuf,
  last_saved: Option<Session>,
}

impl SessionStore {
  pub fn new(config_path: &Path) -> Self {
    let path = config_path
      .parent()
      .unwrap_or_else(|| Path::new("."))
      .join(SESSION_FILE_NAME);

    Self {
      path,
      last_saved: None,
    }
  }

  pub fn load(&self) -> Option<Session> {
    let contents = fs::read_to_string(&self.path).ok()?;

    serde_json::from_str(&contents)
      .inspect_err(|err| tracing::warn!("Ignoring invalid session: {err}"))
      .ok()
  }

  pub fn save(&mut self, workspaces: &[WorkspaceDto]) -> anyhow::Result<()> {
    let session = Session::from_workspaces(workspaces);

    if session.workspaces.is_empty()
      || self.last_saved.as_ref() == Some(&session)
    {
      return Ok(());
    }

    let temp_path = self.path.with_extension("json.tmp");
    fs::write(&temp_path, serde_json::to_string_pretty(&session)?)?;
    fs::rename(&temp_path, &self.path)?;

    self.last_saved = Some(session);
    Ok(())
  }
}

impl Session {
  fn from_workspaces(workspaces: &[WorkspaceDto]) -> Self {
    Self {
      focused_workspace: workspaces
        .iter()
        .find(|workspace| workspace.has_focus)
        .map(|workspace| workspace.name.clone()),
      workspaces: workspaces
        .iter()
        .map(|workspace| {
          let mut windows = Vec::new();
          collect_windows(&workspace.children, &mut windows);

          SessionWorkspace {
            name: workspace.name.clone(),
            tiling_direction: workspace.tiling_direction.clone(),
            is_accordion: workspace.is_accordion,
            windows: windows.into_iter().map(SessionWindow::from).collect(),
          }
        })
        .collect(),
    }
  }
}

impl From<&WindowDto> for SessionWindow {
  fn from(window: &WindowDto) -> Self {
    Self {
      handle: window.handle,
      process_name: window.process_name.clone(),
      class_name: window.class_name.clone(),
      title: window.title.clone(),
    }
  }
}

fn collect_windows<'a>(
  children: &'a [ContainerDto],
  windows: &mut Vec<&'a WindowDto>,
) {
  for child in children {
    match child {
      ContainerDto::Window(window) => windows.push(window),
      ContainerDto::Split(split) => collect_windows(&split.children, windows),
      ContainerDto::Root(_)
      | ContainerDto::Monitor(_)
      | ContainerDto::Workspace(_) => {}
    }
  }
}

/// Moves managed windows back to their saved workspaces and reapplies
/// each workspace's layout.
///
/// Windows are matched by handle first (survives WM restarts), then by
/// process, class and title (survives app restarts with stable titles).
pub fn restore_session(
  session: &Session,
  wm: &mut WindowManager,
  config: &mut UserConfig,
) -> anyhow::Result<()> {
  let managed_windows = managed_window_dtos(wm)?;
  let configured_workspaces = config
    .value
    .workspaces
    .iter()
    .map(|workspace| workspace.name.clone())
    .collect::<HashSet<_>>();

  let mut claimed_window_ids = HashSet::new();

  for saved_workspace in &session.workspaces {
    if !configured_workspaces.contains(&saved_workspace.name) {
      continue;
    }

    for saved_window in &saved_workspace.windows {
      let Some(window_id) = find_matching_window(
        saved_window,
        &managed_windows,
        &claimed_window_ids,
      ) else {
        continue;
      };

      claimed_window_ids.insert(window_id);

      if let Err(err) = wm.process_commands(
        &vec![move_to_workspace_command(&saved_workspace.name)],
        Some(window_id),
        config,
      ) {
        tracing::warn!("Failed to restore window to workspace: {err}");
      }
    }
  }

  for saved_workspace in &session.workspaces {
    restore_workspace_layout(saved_workspace, wm, config);
  }

  if let Some(focused_workspace) = &session.focused_workspace {
    if configured_workspaces.contains(focused_workspace) {
      wm.process_commands(
        &vec![focus_workspace_command(focused_workspace)],
        None,
        config,
      )?;
    }
  }

  tracing::info!(
    "Restored {} windows from the saved session.",
    claimed_window_ids.len()
  );

  Ok(())
}

fn managed_window_dtos(
  wm: &WindowManager,
) -> anyhow::Result<Vec<(Uuid, WindowDto)>> {
  let mut windows = Vec::new();

  for window in wm.state.windows() {
    if let ContainerDto::Window(window_dto) = window.to_dto()? {
      windows.push((window_dto.id, window_dto));
    }
  }

  Ok(windows)
}

fn find_matching_window(
  saved_window: &SessionWindow,
  managed_windows: &[(Uuid, WindowDto)],
  claimed_window_ids: &HashSet<Uuid>,
) -> Option<Uuid> {
  let unclaimed = || {
    managed_windows
      .iter()
      .filter(|(id, _)| !claimed_window_ids.contains(id))
  };

  unclaimed()
    .find(|(_, window)| window.handle == saved_window.handle)
    .or_else(|| {
      unclaimed().find(|(_, window)| {
        window.process_name == saved_window.process_name
          && window.class_name == saved_window.class_name
          && window.title == saved_window.title
      })
    })
    .map(|(id, _)| *id)
}

fn restore_workspace_layout(
  saved_workspace: &SessionWorkspace,
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

  let mut commands = Vec::new();

  if workspace.tiling_direction() != saved_workspace.tiling_direction {
    commands.push(InvokeCommand::SetTilingDirection {
      tiling_direction: saved_workspace.tiling_direction.clone(),
    });
  }

  if workspace.is_accordion() != saved_workspace.is_accordion {
    commands.push(InvokeCommand::ToggleAccordion);
  }

  if commands.is_empty() {
    return;
  }

  if let Err(err) =
    wm.process_commands(&commands, Some(workspace.id()), config)
  {
    tracing::warn!("Failed to restore workspace layout: {err}");
  }
}

fn move_to_workspace_command(workspace: &str) -> InvokeCommand {
  InvokeCommand::Move(InvokeMoveCommand {
    direction: None,
    workspace_in_direction: None,
    workspace: Some(workspace.to_string()),
    next_active_workspace: false,
    prev_active_workspace: false,
    next_workspace: false,
    prev_workspace: false,
    next_active_workspace_on_monitor: false,
    prev_active_workspace_on_monitor: false,
    recent_workspace: false,
  })
}

fn focus_workspace_command(workspace: &str) -> InvokeCommand {
  InvokeCommand::Focus(InvokeFocusCommand {
    direction: None,
    container_id: None,
    workspace_in_direction: None,
    workspace: Some(workspace.to_string()),
    monitor: None,
    next_active_workspace: false,
    prev_active_workspace: false,
    next_workspace: false,
    prev_workspace: false,
    next_active_workspace_on_monitor: false,
    prev_active_workspace_on_monitor: false,
    recent_workspace: false,
  })
}
