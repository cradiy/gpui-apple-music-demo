use super::*;

#[derive(Default)]
pub(super) struct OutputState {
    pub snapshot: AudioOutputState,
    pub error: Option<String>,
    pub subscription: Option<Subscription>,
}

impl MobileApp {
    pub(super) fn observe_outputs(&mut self, cx: &mut Context<Self>) {
        self.output = OutputState::default();
        let weak = cx.entity().downgrade();
        match cx.observe_audio_outputs(move |result, cx| {
            let _ = weak.update(cx, |this, cx| {
                match result {
                    Ok(snapshot) => {
                        this.output.snapshot = snapshot;
                        this.output.error = None;
                    }
                    Err(error) => this.output.error = Some(error.to_string()),
                }
                modal::layer(cx).update(cx, |_, cx| cx.notify());
            });
        }) {
            Ok(subscription) => self.output.subscription = Some(subscription),
            Err(error) => self.output.error = Some(error.to_string()),
        }
    }

    pub(super) fn select_output(&mut self, id: &str, cx: &mut Context<Self>) {
        self.output.error = cx
            .select_audio_output(id)
            .err()
            .map(|error| error.to_string());
        modal::layer(cx).update(cx, |_, cx| cx.notify());
    }

    pub(super) fn allow_bluetooth_outputs(&mut self, cx: &mut Context<Self>) {
        let authorize = cx.authorize_audio_outputs();
        cx.spawn(async move |this, cx| {
            if let Err(error) = authorize.await {
                let _ = this.update(cx, |this, cx| {
                    this.output.error = Some(error.to_string());
                    modal::layer(cx).update(cx, |_, cx| cx.notify());
                });
            }
        })
        .detach();
    }
}
