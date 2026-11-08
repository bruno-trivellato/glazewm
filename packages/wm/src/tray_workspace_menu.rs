use tray_icon::menu::{
  IsMenuItem, MenuItem, PredefinedMenuItem, Submenu,
};
use wm_common::{
  ContainerDto, TilingDirection, WindowDto, WindowState, WorkspaceDto,
};

use crate::sys_tray::TrayMenuId;

const MAX_TITLE_CHARS: usize = 48;
const NESTING_INDENT: &str = "    ";

/// Builds one tray submenu per workspace, listing its layout and windows.
pub fn build_workspace_submenus(
  workspaces: &[WorkspaceDto],
) -> anyhow::Result<Vec<Submenu>> {
  workspaces.iter().map(build_workspace_submenu).collect()
}

fn build_workspace_submenu(
  workspace: &WorkspaceDto,
) -> anyhow::Result<Submenu> {
  let submenu = Submenu::new(workspace_label(workspace), true);

  submenu.append(&MenuItem::with_id(
    TrayMenuId::FocusWorkspace(workspace.name.clone()),
    "Go to workspace",
    !workspace.has_focus,
    None,
  ))?;

  submenu.append(&MenuItem::with_id(
    TrayMenuId::ToggleWorkspaceTilingDirection(workspace.id),
    format!(
      "Switch layout to {}",
      direction_name(&workspace.tiling_direction.inverse())
    ),
    true,
    None,
  ))?;

  submenu.append(&MenuItem::with_id(
    TrayMenuId::ToggleWorkspaceAccordion(workspace.id),
    if workspace.is_accordion {
      "Switch to tiles"
    } else {
      "Switch to accordion"
    },
    true,
    None,
  ))?;

  submenu.append(&PredefinedMenuItem::separator())?;

  let mut container_items = Vec::new();
  collect_container_items(&workspace.children, 0, &mut container_items);

  if container_items.is_empty() {
    container_items.push(MenuItem::new("No windows", false, None));
  }

  let item_refs = container_items
    .iter()
    .map(|item| item as &dyn IsMenuItem)
    .collect::<Vec<_>>();
  submenu.append_items(&item_refs)?;

  Ok(submenu)
}

fn collect_container_items(
  children: &[ContainerDto],
  depth: usize,
  items: &mut Vec<MenuItem>,
) {
  let indent = NESTING_INDENT.repeat(depth);

  for child in children {
    match child {
      ContainerDto::Window(window) => {
        items.push(MenuItem::with_id(
          TrayMenuId::FocusWindow(window.id),
          format!("{indent}{}", window_label(window)),
          true,
          None,
        ));
      }
      ContainerDto::Split(split) => {
        items.push(MenuItem::new(
          format!(
            "{indent}{} group",
            layout_name(&split.tiling_direction, split.is_accordion)
          ),
          false,
          None,
        ));
        collect_container_items(&split.children, depth + 1, items);
      }
      ContainerDto::Root(_)
      | ContainerDto::Monitor(_)
      | ContainerDto::Workspace(_) => {}
    }
  }
}

fn workspace_label(workspace: &WorkspaceDto) -> String {
  let focus_marker = if workspace.has_focus { "▶ " } else { "" };
  let name = workspace.display_name.as_ref().unwrap_or(&workspace.name);
  let window_count = count_windows(&workspace.children);

  format!(
    "{focus_marker}{name}  ·  {}  ·  {window_count} {}",
    layout_name(&workspace.tiling_direction, workspace.is_accordion),
    if window_count == 1 { "window" } else { "windows" }
  )
}

fn window_label(window: &WindowDto) -> String {
  let focus_marker = if window.has_focus { "• " } else { "" };
  let title = truncate(&window.title, MAX_TITLE_CHARS);

  format!(
    "{focus_marker}{}: {title}  [{}]",
    window.process_name,
    state_name(&window.state)
  )
}

fn count_windows(children: &[ContainerDto]) -> usize {
  children
    .iter()
    .map(|child| match child {
      ContainerDto::Window(_) => 1,
      ContainerDto::Split(split) => count_windows(&split.children),
      ContainerDto::Root(_)
      | ContainerDto::Monitor(_)
      | ContainerDto::Workspace(_) => 0,
    })
    .sum()
}

fn direction_name(direction: &TilingDirection) -> &'static str {
  match direction {
    TilingDirection::Horizontal => "horizontal",
    TilingDirection::Vertical => "vertical",
  }
}

fn layout_name(
  direction: &TilingDirection,
  is_accordion: bool,
) -> String {
  if is_accordion {
    format!("{} accordion", direction_name(direction))
  } else {
    direction_name(direction).to_string()
  }
}

fn state_name(state: &WindowState) -> &'static str {
  match state {
    WindowState::Tiling => "tiling",
    WindowState::Floating(_) => "floating",
    WindowState::Fullscreen(_) => "fullscreen",
    WindowState::Minimized => "minimized",
  }
}

fn truncate(text: &str, max_chars: usize) -> String {
  if text.chars().count() <= max_chars {
    text.to_string()
  } else {
    let kept = text.chars().take(max_chars - 1).collect::<String>();
    format!("{kept}…")
  }
}
