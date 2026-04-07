use crate::config::Config;
use crate::ui::file_browser::{FileBrowser, FolderAction};
use anyhow::Result;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub enum AppMode {
    Normal,
    ExportPopup,
    ConfirmDialog { message: String, on_confirm: ConfirmAction },
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConfirmAction {
    WriteBack,
    DeleteOriginals,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ActivePanel {
    Browser,
    Actions,
}

pub struct App {
    pub config: Config,
    pub mode: AppMode,
    pub active_panel: ActivePanel,
    pub browser: FileBrowser,
    pub status_messages: Vec<String>,
    pub should_quit: bool,
    pub working_dir: PathBuf,
    /// Currently highlighted action in the right panel
    pub action_cursor: usize,
}

impl App {
    pub fn new(config: Config) -> Result<Self> {
        let source_dir = config.resolve_source_dir();
        let working_dir = config.resolve_working_dir();

        let browser = FileBrowser::new(source_dir.clone())?;

        Ok(Self {
            config,
            mode: AppMode::Normal,
            active_panel: ActivePanel::Browser,
            browser,
            status_messages: vec![
                "Welcome to musicman! Navigate with hjkl, Tab to switch panels.".into(),
                format!("Source: {}", source_dir.display()),
                format!("Working copy: {}", working_dir.display()),
            ],
            should_quit: false,
            working_dir,
            action_cursor: 0,
        })
    }

    pub fn push_status(&mut self, msg: impl Into<String>) {
        self.status_messages.push(msg.into());
        if self.status_messages.len() > 100 {
            self.status_messages.remove(0);
        }
    }

    pub fn handle_key(&mut self, key: char) -> Result<()> {
        match &self.mode {
            AppMode::Normal => self.handle_normal_key(key),
            AppMode::ExportPopup => self.handle_export_key(key),
            AppMode::ConfirmDialog { .. } => self.handle_confirm_key(key),
        }
    }

    fn handle_normal_key(&mut self, key: char) -> Result<()> {
        let kb = self.config.keybinds.clone();

        if key == kb.quit {
            self.should_quit = true;
            return Ok(());
        }

        // Tab always switches panels regardless of which is active
        if key == '\t' {
            self.active_panel = match self.active_panel {
                ActivePanel::Browser => ActivePanel::Actions,
                ActivePanel::Actions => ActivePanel::Browser,
            };
            return Ok(());
        }

        match self.active_panel {
            ActivePanel::Browser => {
                if key == kb.navigate_up {
                    self.browser.move_up();
                    self.action_cursor = 0;
                } else if key == kb.navigate_down {
                    self.browser.move_down();
                    self.action_cursor = 0;
                } else if key == kb.expand {
                    let before = self.browser.nodes.len();
                    self.browser.expand_selected();
                    let after = self.browser.nodes.len();
                    if after > before {
                        self.push_status(format!(
                            "Expanded: {} ({} items)",
                            self.browser.nodes.get(self.browser.selected)
                                .map(|n| n.name.as_str()).unwrap_or("?"),
                            after - before
                        ));
                    }
                } else if key == kb.collapse {
                    self.browser.collapse_selected();
                    self.push_status("Collapsed.");
                }
            }
            ActivePanel::Actions => {
                let actions = self.selected_actions();
                if actions.is_empty() {
                    return Ok(());
                }

                if key == kb.navigate_up {
                    if self.action_cursor > 0 {
                        self.action_cursor -= 1;
                    }
                } else if key == kb.navigate_down {
                    if self.action_cursor + 1 < actions.len() {
                        self.action_cursor += 1;
                    }
                } else if key == '\r' {
                    // Enter fires the highlighted action
                    if let Some(action) = actions.get(self.action_cursor) {
                        self.fire_action(action.clone())?;
                    }
                } else {
                    // Direct keybind shortcuts work from either panel
                    let action = match key {
                        'f' => Some(FolderAction::FetchMetadata),
                        'c' => Some(FolderAction::CleanMetadata),
                        'v' => Some(FolderAction::ConvertToM4A),
                        'd' => Some(FolderAction::DateOrderTracknumbers),
                        'e' => Some(FolderAction::ExportToIpod),
                        'w' => Some(FolderAction::WriteBack),
                        _ => None,
                    };
                    if let Some(a) = action {
                        self.fire_action(a)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn fire_action(&mut self, action: FolderAction) -> Result<()> {
        let path = match self.browser.selected_path() {
            Some(p) => p.clone(),
            None => {
                self.push_status("No file or folder selected.");
                return Ok(());
            }
        };

        match action {
            FolderAction::FetchMetadata => {
                self.push_status(format!("[ ] Fetching metadata for {} ...", path.file_name().unwrap_or_default().to_string_lossy()));
                // TODO: async metadata fetch
                self.push_status("[!] Metadata fetch not yet wired to async runtime.");
            }
            FolderAction::CleanMetadata => {
                self.push_status(format!("[ ] Cleaning metadata: {}", path.display()));
                match crate::metadata::cleaner::clean_directory(&path) {
                    Ok(cleaned) => self.push_status(format!("[✓] Cleaned {} files.", cleaned.len())),
                    Err(e) => self.push_status(format!("[✗] Clean failed: {}", e)),
                }
            }
            FolderAction::ConvertToM4A => {
                if !self.config.conversion.enabled {
                    self.push_status("[!] Conversion is disabled in config.toml (conversion.enabled = false). Enable it first.");
                    return Ok(());
                }
                let bitrate = self.config.conversion.bitrate.clone();
                self.push_status(format!("[ ] Converting FLAC→M4A in {} ...", path.file_name().unwrap_or_default().to_string_lossy()));
                match crate::audio::converter::convert_directory(&path, &bitrate, false) {
                    Ok(results) => self.push_status(format!("[✓] Converted {} files.", results.len())),
                    Err(e) => self.push_status(format!("[✗] Conversion failed: {}", e)),
                }
            }
            FolderAction::DateOrderTracknumbers => {
                // Toggle the flag on the node, then apply if turning on
                self.browser.toggle_date_order();
                let enabled = self.browser.nodes.get(self.browser.selected)
                    .map(|n| n.date_order_enabled)
                    .unwrap_or(false);
                if enabled {
                    self.push_status(format!("[ ] Applying date-order track numbers to {} ...", path.file_name().unwrap_or_default().to_string_lossy()));
                    match crate::metadata::cleaner::apply_date_order_track_numbers(&path) {
                        Ok(results) => self.push_status(format!("[✓] Numbered {} tracks by date.", results.len())),
                        Err(e) => self.push_status(format!("[✗] Date-order failed: {}", e)),
                    }
                } else {
                    self.push_status("Date-order track numbering disabled for this folder.");
                }
            }
            FolderAction::ExportToIpod => {
                self.mode = AppMode::ExportPopup;
            }
            FolderAction::WriteBack => {
                self.mode = AppMode::ConfirmDialog {
                    message: format!("Write working copy back to source? This modifies original files."),
                    on_confirm: ConfirmAction::WriteBack,
                };
            }
        }
        Ok(())
    }

    fn handle_export_key(&mut self, key: char) -> Result<()> {
        if key == 'q' || key == '\x1b' {
            self.mode = AppMode::Normal;
        }
        Ok(())
    }

    fn handle_confirm_key(&mut self, key: char) -> Result<()> {
        match key {
            'y' | 'Y' => {
                if let AppMode::ConfirmDialog { on_confirm, .. } = self.mode.clone() {
                    self.execute_confirm(on_confirm)?;
                }
                self.mode = AppMode::Normal;
            }
            'n' | 'N' | '\x1b' => {
                self.mode = AppMode::Normal;
                self.push_status("Cancelled.");
            }
            _ => {}
        }
        Ok(())
    }

    fn execute_confirm(&mut self, action: ConfirmAction) -> Result<()> {
        match action {
            ConfirmAction::WriteBack => {
                self.push_status("[!] Write-back not yet implemented.");
            }
            ConfirmAction::DeleteOriginals => {
                self.push_status("[!] Delete originals not yet implemented.");
            }
        }
        Ok(())
    }

    pub fn open_export_popup(&mut self) {
        self.mode = AppMode::ExportPopup;
    }

    pub fn selected_path(&self) -> Option<&PathBuf> {
        self.browser.selected_path()
    }

    pub fn selected_actions(&self) -> Vec<FolderAction> {
        if let Some(path) = self.browser.selected_path() {
            FolderAction::available_for(path)
        } else {
            vec![]
        }
    }
}
