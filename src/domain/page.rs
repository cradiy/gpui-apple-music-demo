use gpui::SharedString;
use serde::Serialize;

#[derive(Clone, PartialEq, Eq, Serialize)]
pub enum Page {
    Search,
    Home,
    RecentlyAdded,
    Artists,
    Artist(SharedString),
    Albums,
    Album(SharedString),
    Songs,
    PlayLists(PlayList),
}

#[derive(Clone, PartialEq, Eq, Serialize)]
pub enum PlayList {
    All,
    Like,
    Custom(SharedString),
}
