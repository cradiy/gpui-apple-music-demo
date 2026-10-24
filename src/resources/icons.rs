use gpui::SharedString;
use gpui_icons::file_enum;

#[file_enum(
    ext = "svg",
    rename_all = "PascalCase",
    group(path = "assets/icons", asset_prefix = "icons")
)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RomusIcons {}

impl From<RomusIcons> for SharedString {
    fn from(icon: RomusIcons) -> Self {
        SharedString::from(icon.path())
    }
}
