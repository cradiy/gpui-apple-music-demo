use gpui::SharedString;

use crate::domain::Page;

pub struct NavigationState {
    pub page: Page,
    pub search_query: SharedString,
}

impl Default for NavigationState {
    fn default() -> Self {
        Self {
            page: Page::Home,
            search_query: "".into(),
        }
    }
}
