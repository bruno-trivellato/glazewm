use std::{
  fmt::{self, Display},
  path::Path,
  str::FromStr,
  sync::{Arc, Mutex},
};

use anyhow::Context;
use auto_launch::AutoLaunch;
use tokio::sync::mpsc;
use tray_icon::{
  menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem},
  Icon, TrayIcon, TrayIconBuilder,
};
use uuid::Uuid;
use wm_common::{InvokeCommand, InvokeFocusCommand, WorkspaceDto};
#[cfg(target_os = "windows")]
use wm_platform::DispatcherExtWindows;
use wm_platform::{Dispatcher, ThreadBound};

use crate::tray_workspace_menu::build_workspace_submenus;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum TrayMenuId {
  FocusWorkspace(String),
  FocusWindow(Uuid),
  ToggleWorkspaceTilingDirection(Uuid),
  ReloadConfig,
  ShowConfigFolder,
  #[cfg(target_os = "windows")]
  ToggleWindowAnimations,
  RunOnStartup,
  Exit,
}

impl Display for TrayMenuId {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      TrayMenuId::FocusWorkspace(name) => {
        write!(f, "focus_workspace:{name}")
      }
      TrayMenuId::FocusWindow(id) => write!(f, "focus_window:{id}"),
      TrayMenuId::ToggleWorkspaceTilingDirection(id) => {
        write!(f, "toggle_workspace_tiling_direction:{id}")
      }
      TrayMenuId::ReloadConfig => write!(f, "reload_config"),
      TrayMenuId::ShowConfigFolder => write!(f, "show_config_folder"),
      #[cfg(target_os = "windows")]
      TrayMenuId::ToggleWindowAnimations => {
        write!(f, "toggle_window_animations")
      }
      TrayMenuId::RunOnStartup => write!(f, "run_on_startup"),
      TrayMenuId::Exit => write!(f, "exit"),
    }
  }
}

impl FromStr for TrayMenuId {
  type Err = anyhow::Error;

  fn from_str(event: &str) -> Result<Self, Self::Err> {
    if let Some((kind, value)) = event.split_once(':') {
      return match kind {
        "focus_workspace" => Ok(Self::FocusWorkspace(value.to_string())),
        "focus_window" => Ok(Self::FocusWindow(Uuid::parse_str(value)?)),
        "toggle_workspace_tiling_direction" => {
          Ok(Self::ToggleWorkspaceTilingDirection(Uuid::parse_str(value)?))
        }
        _ => anyhow::bail!("Invalid tray menu event: {}", event),
      };
    }

    match event {
      "show_config_folder" => Ok(Self::ShowConfigFolder),
      "reload_config" => Ok(Self::ReloadConfig),
      #[cfg(target_os = "windows")]
      "toggle_window_animations" => Ok(Self::ToggleWindowAnimations),
      "run_on_startup" => Ok(Self::RunOnStartup),
      "exit" => Ok(Self::Exit),
      _ => anyhow::bail!("Invalid tray menu event: {}", event),
    }
  }
}

/// WM commands requested from the tray menu, run against an optional
/// subject container.
pub struct TrayCommand {
  pub commands: Vec<InvokeCommand>,
  pub subject_container_id: Option<Uuid>,
}

pub struct SystemTray {
  pub config_reload_rx: mpsc::UnboundedReceiver<()>,
  pub exit_rx: mpsc::UnboundedReceiver<()>,
  pub command_rx: mpsc::UnboundedReceiver<TrayCommand>,
  _icon_thread: Option<std::thread::JoinHandle<()>>,
  _tray_icon: ThreadBound<TrayIcon>,
  tray_menu: ThreadBound<Menu>,
  workspace_item_count: usize,
}

impl SystemTray {
  /// Install the system tray on the main thread after the run loop starts.
  pub fn new(
    config_path: &Path,
    dispatcher: Dispatcher,
  ) -> anyhow::Result<Self> {
    let (exit_tx, exit_rx) = mpsc::unbounded_channel();
    let (config_reload_tx, config_reload_rx) = mpsc::unbounded_channel();
    let (command_tx, command_rx) = mpsc::unbounded_channel();

    let animations_enabled = Arc::new(Mutex::new({
      #[cfg(target_os = "windows")]
      {
        dispatcher.window_animations_enabled().unwrap_or(false)
      }
      #[cfg(not(target_os = "windows"))]
      {
        false
      }
    }));

    let run_on_startup_enabled = Arc::new(Mutex::new(
      auto_launch_instance()
        .and_then(|auto_launch| {
          auto_launch.is_enabled().map_err(Into::into)
        })
        .unwrap_or(false),
    ));

    let (tray_icon, tray_menu) = dispatcher.dispatch_sync(|| {
      let (tray_icon, tray_menu) = Self::create_tray_icon(
        *animations_enabled.lock().unwrap(),
        *run_on_startup_enabled.lock().unwrap(),
      )
      .unwrap();
      (
        ThreadBound::new(tray_icon, dispatcher.clone()),
        ThreadBound::new(tray_menu, dispatcher.clone()),
      )
    })?;

    // Spawn thread to handle tray menu events.
    let config_path = config_path.to_owned();
    let icon_thread = std::thread::spawn(move || {
      let menu_event_rx = MenuEvent::receiver();

      while let Ok(event) = menu_event_rx.recv() {
        if let Ok(menu_event) = TrayMenuId::from_str(event.id.as_ref()) {
          if let Err(err) = Self::handle_menu_event(
            &menu_event,
            &dispatcher,
            &config_path,
            &config_reload_tx,
            &exit_tx,
            &command_tx,
            &animations_enabled,
            &run_on_startup_enabled,
          ) {
            tracing::warn!("Failed to handle tray menu event: {}", err);
          }
        }
      }
    });

    Ok(Self {
      config_reload_rx,
      exit_rx,
      command_rx,
      _icon_thread: Some(icon_thread),
      _tray_icon: tray_icon,
      tray_menu,
      workspace_item_count: 0,
    })
  }

  /// Replaces the workspace submenus at the top of the tray menu.
  pub fn update_workspaces(
    &mut self,
    workspaces: Vec<WorkspaceDto>,
  ) -> anyhow::Result<()> {
    let previous_item_count = self.workspace_item_count;

    self.workspace_item_count = self.tray_menu.with(move |menu| {
      for _ in 0..previous_item_count {
        menu.remove_at(0);
      }

      let submenus = build_workspace_submenus(&workspaces)?;
      for (position, submenu) in submenus.iter().enumerate() {
        menu.insert(submenu, position)?;
      }

      let separator_position = submenus.len();
      if separator_position > 0 {
        menu.insert(&PredefinedMenuItem::separator(), separator_position)?;
        return anyhow::Ok(separator_position + 1);
      }

      anyhow::Ok(0)
    })??;

    Ok(())
  }

  fn create_tray_icon(
    // LINT: `animations_enabled` is only used on Windows.
    #[cfg_attr(not(target_os = "windows"), allow(unused_variables))]
    animations_enabled: bool,
    run_on_startup_enabled: bool,
  ) -> anyhow::Result<(TrayIcon, Menu)> {
    let reload_config_item = MenuItem::with_id(
      TrayMenuId::ReloadConfig,
      "Reload config",
      true,
      None,
    );

    let config_dir_item = MenuItem::with_id(
      TrayMenuId::ShowConfigFolder,
      "Show config folder",
      true,
      None,
    );

    #[cfg(target_os = "windows")]
    let toggle_animations_item = CheckMenuItem::with_id(
      TrayMenuId::ToggleWindowAnimations,
      "Window animations",
      true,
      animations_enabled,
      None,
    );

    let run_on_startup_item = CheckMenuItem::with_id(
      TrayMenuId::RunOnStartup,
      "Run on system startup",
      true,
      run_on_startup_enabled,
      None,
    );

    let exit_item =
      MenuItem::with_id(TrayMenuId::Exit, "Exit", true, None);

    let tray_menu = Menu::new();
    tray_menu.append_items(&[
      &reload_config_item,
      &config_dir_item,
      #[cfg(target_os = "windows")]
      &toggle_animations_item,
      &run_on_startup_item,
      &PredefinedMenuItem::separator(),
      &exit_item,
    ])?;

    let icon = Self::load_icon(include_bytes!(
      "../../../resources/assets/icon.png"
    ))?;

    let tray_icon = TrayIconBuilder::new()
      .with_menu(Box::new(tray_menu.clone()))
      .with_tooltip(format!("GlazeWM v{}", env!("VERSION_NUMBER")))
      .with_icon(icon)
      .build()?;

    Ok((tray_icon, tray_menu))
  }

  fn load_icon(bytes: &[u8]) -> anyhow::Result<Icon> {
    let (icon_rgba, icon_width, icon_height) = {
      let image = image::load_from_memory(bytes)
        .context("Failed to to create tray icon image from resource.")?
        .into_rgba8();

      let (width, height) = image.dimensions();
      let rgba = image.into_raw();
      (rgba, width, height)
    };

    Ok(tray_icon::Icon::from_rgba(
      icon_rgba,
      icon_width,
      icon_height,
    )?)
  }

  fn handle_menu_event(
    menu_id: &TrayMenuId,
    dispatcher: &Dispatcher,
    config_path: &Path,
    config_reload_tx: &mpsc::UnboundedSender<()>,
    exit_tx: &mpsc::UnboundedSender<()>,
    command_tx: &mpsc::UnboundedSender<TrayCommand>,
    // LINT: `animations_enabled` is only used on Windows.
    #[cfg_attr(not(target_os = "windows"), allow(unused_variables))]
    animations_enabled: &Arc<Mutex<bool>>,
    run_on_startup_enabled: &Arc<Mutex<bool>>,
  ) -> anyhow::Result<()> {
    tracing::info!("Processing tray menu event: {:?}", menu_id);

    match menu_id {
      TrayMenuId::FocusWorkspace(name) => {
        command_tx.send(TrayCommand {
          commands: vec![focus_command(None, Some(name.clone()))],
          subject_container_id: None,
        })?;
        Ok(())
      }
      TrayMenuId::FocusWindow(window_id) => {
        command_tx.send(TrayCommand {
          commands: vec![focus_command(Some(*window_id), None)],
          subject_container_id: None,
        })?;
        Ok(())
      }
      TrayMenuId::ToggleWorkspaceTilingDirection(workspace_id) => {
        command_tx.send(TrayCommand {
          commands: vec![InvokeCommand::ToggleTilingDirection],
          subject_container_id: Some(*workspace_id),
        })?;
        Ok(())
      }
      TrayMenuId::ShowConfigFolder => {
        dispatcher.open_file_explorer({
          #[cfg(target_os = "windows")]
          {
            config_path.parent().context("Invalid config path.")?
          }
          #[cfg(target_os = "macos")]
          {
            // On macOS, pass the file path directly since Finder
            // navigates one level too high with the parent directory.
            config_path
          }
        })?;

        Ok(())
      }
      TrayMenuId::ReloadConfig => {
        config_reload_tx.send(())?;
        Ok(())
      }
      #[cfg(target_os = "windows")]
      TrayMenuId::ToggleWindowAnimations => {
        let mut animations_enabled = animations_enabled.lock().unwrap();
        dispatcher.set_window_animations_enabled(!*animations_enabled)?;
        *animations_enabled = !*animations_enabled;
        Ok(())
      }
      TrayMenuId::RunOnStartup => {
        let mut run_on_startup_enabled =
          run_on_startup_enabled.lock().unwrap();

        if *run_on_startup_enabled {
          auto_launch_instance()?.disable()?;
        } else {
          auto_launch_instance()?.enable()?;
        }

        *run_on_startup_enabled = !*run_on_startup_enabled;
        Ok(())
      }
      TrayMenuId::Exit => {
        exit_tx.send(())?;
        Ok(())
      }
    }
  }
}

/// Builds a `focus` command targeting either a container or a workspace.
fn focus_command(
  container_id: Option<Uuid>,
  workspace: Option<String>,
) -> InvokeCommand {
  InvokeCommand::Focus(InvokeFocusCommand {
    direction: None,
    container_id,
    workspace_in_direction: None,
    workspace,
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

/// Creates a new [`AutoLaunch`] instance for managing auto-launch at
/// system startup.
fn auto_launch_instance() -> anyhow::Result<AutoLaunch> {
  let exe_path = std::env::current_exe()?.to_string_lossy().to_string();
  let args: [&str; 0] = [];

  #[cfg(target_os = "windows")]
  let instance = AutoLaunch::new("GlazeWM", &exe_path, &args);

  #[cfg(target_os = "macos")]
  let instance = AutoLaunch::new("GlazeWM", &exe_path, false, &args);

  Ok(instance)
}
