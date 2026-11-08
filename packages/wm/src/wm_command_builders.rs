use uuid::Uuid;
use wm_common::{InvokeCommand, InvokeFocusCommand, InvokeMoveCommand};

pub fn focus_window(window_id: Uuid) -> InvokeCommand {
  InvokeCommand::Focus(InvokeFocusCommand {
    container_id: Some(window_id),
    ..focus_command_without_target()
  })
}

pub fn focus_workspace(workspace_name: &str) -> InvokeCommand {
  InvokeCommand::Focus(InvokeFocusCommand {
    workspace: Some(workspace_name.to_string()),
    ..focus_command_without_target()
  })
}

pub fn move_to_workspace(workspace_name: &str) -> InvokeCommand {
  InvokeCommand::Move(InvokeMoveCommand {
    workspace: Some(workspace_name.to_string()),
    ..move_command_without_target()
  })
}

fn focus_command_without_target() -> InvokeFocusCommand {
  InvokeFocusCommand {
    direction: None,
    container_id: None,
    workspace_in_direction: None,
    workspace: None,
    monitor: None,
    next_active_workspace: false,
    prev_active_workspace: false,
    next_workspace: false,
    prev_workspace: false,
    next_active_workspace_on_monitor: false,
    prev_active_workspace_on_monitor: false,
    recent_workspace: false,
  }
}

fn move_command_without_target() -> InvokeMoveCommand {
  InvokeMoveCommand {
    direction: None,
    workspace_in_direction: None,
    workspace: None,
    next_active_workspace: false,
    prev_active_workspace: false,
    next_workspace: false,
    prev_workspace: false,
    next_active_workspace_on_monitor: false,
    prev_active_workspace_on_monitor: false,
    recent_workspace: false,
  }
}
