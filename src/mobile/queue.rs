use super::*;
use std::collections::HashMap;
use uic::components::{
    reorderable_list::ReorderState,
    swipe_actions::{SwipeActionsState, SwipeDirection, SwipeTriggered},
};

#[derive(Clone)]
pub(super) struct QueueEntry {
    pub key: SharedString,
    pub track_id: String,
}

impl QueueEntry {
    pub fn new(track_id: String) -> Self {
        Self {
            key: uuid::Uuid::new_v4().to_string().into(),
            track_id,
        }
    }
}

pub(super) struct QueueRow {
    pub swipe: Entity<SwipeActionsState>,
    pub removing: bool,
    _subscription: Subscription,
}

pub(super) struct QueueUi {
    pub order: Entity<ReorderState>,
    pub rows: HashMap<SharedString, QueueRow>,
}

impl MobileApp {
    pub(super) fn upcoming_queue(&self) -> Vec<&QueueEntry> {
        self.queue
            .iter()
            .skip(self.queue_position().map_or(0, |index| index + 1))
            .collect()
    }

    pub(super) fn prepare_queue(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ui = self.queue_ui.get_or_insert_with(|| QueueUi {
            order: cx.new(|cx| ReorderState::new(window, cx)),
            rows: HashMap::new(),
        });
        ui.rows
            .retain(|key, _| self.queue.iter().any(|entry| &entry.key == key));
        for entry in &self.queue {
            if ui.rows.contains_key(&entry.key) {
                continue;
            }
            let swipe = cx.new(|cx| SwipeActionsState::new(window, cx));
            let key = entry.key.clone();
            let subscription = cx.subscribe(&swipe, move |this, _, event: &SwipeTriggered, cx| {
                if !this.queue.iter().any(|entry| entry.key == key) {
                    return;
                }
                match event.direction {
                    SwipeDirection::Right => {}
                    SwipeDirection::Left => this.dismiss_queue_entry(&key, cx),
                }
            });
            ui.rows.insert(
                entry.key.clone(),
                QueueRow {
                    swipe,
                    removing: false,
                    _subscription: subscription,
                },
            );
        }
    }

    pub(super) fn dismiss_queue_entry(&mut self, key: &SharedString, cx: &mut Context<Self>) {
        let Some(row) = self.queue_ui.as_mut().and_then(|ui| ui.rows.get_mut(key)) else {
            return;
        };
        if row.removing {
            return;
        }
        row.removing = true;
        let key = key.clone();
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(430))
                .await;
            let _ = this.update(cx, |this, cx| {
                if let Some(index) = this.queue.iter().position(|entry| entry.key == key) {
                    if this.queue_current.as_ref() == Some(&key) {
                        this.queue_current = index
                            .checked_sub(1)
                            .map(|previous| this.queue[previous].key.clone());
                    }
                    this.queue.remove(index);
                }
                if let Some(ui) = &mut this.queue_ui {
                    ui.rows.remove(&key);
                }
                #[cfg(target_os = "android")]
                this.publish_system_media(true, cx);
                cx.notify();
            });
        })
        .detach();
    }
}
