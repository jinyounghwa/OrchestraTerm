use eframe::egui;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Prefix,
    Copy,
    CopySearch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    EnterPrefix,
    EnterCopyMode,
    ExitCopyMode,
    SplitHorizontal,
    SplitVertical,
    ClosePane,
    ToggleZoom,
    FocusPrev,
    FocusNext,
    CopyMoveUp,
    CopyMoveDown,
    CopyMoveLeft,
    CopyMoveRight,
    CopyStartSelection,
    CopyCopySelection,
    CopySearchStart,
    CopySearchApply,
    SendEnter,
    OpenFolder,
}

pub fn map_key(mode: Mode, key: egui::Key, modifiers: egui::Modifiers) -> Option<Action> {
    if modifiers.command && key == egui::Key::O {
        return Some(Action::OpenFolder);
    }

    match mode {
        Mode::Normal => {
            if modifiers.ctrl && key == egui::Key::B {
                return Some(Action::EnterPrefix);
            }
            if modifiers.ctrl && key == egui::Key::Enter {
                return Some(Action::SendEnter);
            }
            None
        }
        Mode::Prefix => match key {
            egui::Key::S => Some(Action::SplitHorizontal),
            egui::Key::V => Some(Action::SplitVertical),
            egui::Key::X => Some(Action::ClosePane),
            egui::Key::Z => Some(Action::ToggleZoom),
            egui::Key::ArrowLeft | egui::Key::ArrowUp => Some(Action::FocusPrev),
            egui::Key::ArrowRight | egui::Key::ArrowDown => Some(Action::FocusNext),
            egui::Key::OpenBracket => Some(Action::EnterCopyMode),
            egui::Key::Escape => Some(Action::ExitCopyMode),
            _ => None,
        },
        Mode::Copy => match key {
            egui::Key::ArrowUp => Some(Action::CopyMoveUp),
            egui::Key::ArrowDown => Some(Action::CopyMoveDown),
            egui::Key::ArrowLeft => Some(Action::CopyMoveLeft),
            egui::Key::ArrowRight => Some(Action::CopyMoveRight),
            egui::Key::Space => Some(Action::CopyStartSelection),
            egui::Key::Enter => Some(Action::CopyCopySelection),
            egui::Key::Slash => Some(Action::CopySearchStart),
            egui::Key::Escape => Some(Action::ExitCopyMode),
            _ => None,
        },
        Mode::CopySearch => match key {
            egui::Key::Enter => Some(Action::CopySearchApply),
            egui::Key::Escape => Some(Action::ExitCopyMode),
            _ => None,
        },
    }
}
