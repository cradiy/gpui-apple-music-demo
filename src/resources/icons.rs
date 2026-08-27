use gpui::SharedString;
use uic::file_enum;

#[file_enum(path = "assets/icons", ext = "svg", rename_all = "PascalCase")]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RomusIcons {}

impl From<RomusIcons> for SharedString {
    fn from(icon: RomusIcons) -> Self {
        SharedString::from(icon.path())
    }
}

impl RomusIcons {
    pub fn path(&self) -> String {
        format!("icons/{self}")
    }
}
