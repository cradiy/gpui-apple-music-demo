use super::{LibraryTab, MobileApp, Page, motion};
use gpui::{AppContext, TestAppContext, px, size};

#[gpui::test]
fn player_transition_reuses_browse_scene_and_return_resumes_rendering(cx: &mut TestAppContext) {
    let handle = cx.open_window(size(px(392.), px(852.)), |_, cx| {
        let mut app = MobileApp::new(cx);
        app.page = Page::Library;
        app
    });
    cx.update_window(handle.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    let scene = handle
        .read_with(cx, |app, _| app.browse_scenes[&Page::Library].clone())
        .unwrap();
    let initial = scene.read_with(cx, |scene, _| scene.renders);
    for _ in 0..8 {
        handle
            .update(cx, |app, window, cx| {
                app.page = Page::NowPlaying;
                app.return_page = Page::Library;
                app.player_dismiss =
                    motion::PlayerDismiss::opening(window.viewport_size().height.as_f32());
                cx.notify();
            })
            .unwrap();
        cx.update_window(handle.into(), |_, window, cx| window.draw(cx).clear())
            .unwrap();
    }
    assert_eq!(scene.read_with(cx, |scene, _| scene.renders), initial + 1);
    for tab in [LibraryTab::Albums, LibraryTab::Songs] {
        let before = scene.read_with(cx, |scene, _| scene.renders);
        handle
            .update(cx, |app, _, cx| {
                app.page = Page::Library;
                app.player_dismiss = motion::PlayerDismiss::default();
                app.library_tab = tab;
                cx.notify();
            })
            .unwrap();
        cx.update_window(handle.into(), |_, window, cx| window.draw(cx).clear())
            .unwrap();
        assert!(scene.read_with(cx, |scene, _| scene.renders) > before);
    }
    handle
        .read_with(cx, |app, _| {
            assert_eq!(app.browse_scenes[&Page::Library], scene);
        })
        .unwrap();
}
