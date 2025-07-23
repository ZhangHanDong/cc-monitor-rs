use makepad_widgets::*;
use crate::i18n;

live_design! {
    use link::theme::*;
    use link::shaders::*;
    use link::widgets::*;

    pub StatusIndicator = {{StatusIndicator}} {
        width: Fill,
        height: Fit,
        flow: Down,
        spacing: 8

        status_row = <View> {
            width: Fill,
            height: Fit,
            flow: Right,
            spacing: 10

            status_dot = <RoundedView> {
                width: 20,
                height: 20,
                show_bg: true,
                draw_bg: {
                    color: #ff0000
                    border_radius: 10.0
                }
            }

            status_text = <Label> {
                text: "状态: 连接失败"
                draw_text: {
                    text_style: {
                        font_size: 14.0
                    }
                    color: #ffffff
                }
            }
        }

        speed_row = <View> {
            width: Fill,
            height: Fit,
            flow: Right,
            spacing: 10
            padding: {left: 30}

            speed_label = <Label> {
                text: "🚀 网速: --"
                draw_text: {
                    text_style: {
                        font_size: 14.0
                    }
                    color: #ffffff
                }
            }
        }

        latency_row = <View> {
            width: Fill,
            height: Fit,
            flow: Right,
            spacing: 10
            padding: {left: 30}

            latency_icon = <Label> {
                text: "🟢"
                draw_text: {
                    text_style: {
                        font_size: 14.0
                    }
                    color: #ffffff
                }
            }

            latency_label = <Label> {
                text: "延迟: --"
                draw_text: {
                    text_style: {
                        font_size: 14.0
                    }
                    color: #ffffff
                }
            }
        }
    }
}

#[derive(Live, LiveHook, Widget)]
pub struct StatusIndicator {
    #[deref] view: View,
    #[rust] connected: bool,
    #[rust] speed: Option<String>,
    #[rust] latency: Option<String>,
}

impl Widget for StatusIndicator {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);

        // Update labels when we redraw
        self.apply_status_updates(cx);
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        // Update status dot color
        let color = if self.connected {
            vec4(0.0, 1.0, 0.0, 1.0) // Green
        } else {
            vec4(1.0, 0.0, 0.0, 1.0) // Red
        };

        self.view(id!(status_dot)).apply_over(cx, live!{
            draw_bg: { color: (color) }
        });

        self.view.draw_walk(cx, scope, walk)
    }
}

impl StatusIndicator {
    pub fn update_status(&mut self, cx: &mut Cx, connected: bool, speed: Option<String>, latency: Option<String>) {
        self.connected = connected;
        self.speed = speed;
        self.latency = latency;
        self.apply_status_updates(cx);
        cx.redraw_all();
    }

    fn apply_status_updates(&mut self, cx: &mut Cx) {
        // Update status text
        if let Some(mut label) = self.view.label(id!(status_text)).borrow_mut() {
            let text = if self.connected {
                format!("{}: {}", i18n::get(i18n::keys::NETWORK_STATUS), i18n::get(i18n::keys::NETWORK_CONNECTED))
            } else {
                format!("{}: {}", i18n::get(i18n::keys::NETWORK_STATUS), i18n::get(i18n::keys::NETWORK_DISCONNECTED))
            };
            label.set_text(cx, &text);
        }

        // Update speed
        if let Some(mut label) = self.view.label(id!(speed_label)).borrow_mut() {
            let text = match &self.speed {
                Some(speed) => format!("{}: {}", i18n::get(i18n::keys::NETWORK_SPEED), speed),
                None => format!("{}: --", i18n::get(i18n::keys::NETWORK_SPEED)),
            };
            label.set_text(cx, &text);
        }

        // Update latency
        if let Some(latency) = &self.latency {
            // Update icon based on latency
            if let Some(mut icon) = self.view.label(id!(latency_icon)).borrow_mut() {
                let icon_text = if let Ok(ms) = latency.trim_end_matches("ms").parse::<f64>() {
                    if ms < 50.0 {
                        "🟢"
                    } else if ms < 150.0 {
                        "🟡"
                    } else {
                        "🔴"
                    }
                } else {
                    "🟢"
                };
                icon.set_text(cx, icon_text);
            }

            // Update latency text
            if let Some(mut label) = self.view.label(id!(latency_label)).borrow_mut() {
                label.set_text(cx, &format!("{}: {}", i18n::get(i18n::keys::NETWORK_LATENCY), latency));
            }
        } else if let Some(mut label) = self.view.label(id!(latency_label)).borrow_mut() {
            label.set_text(cx, &format!("{}: --", i18n::get(i18n::keys::NETWORK_LATENCY)));
        }
    }
    
    pub fn refresh_translations(&mut self, cx: &mut Cx) {
        self.apply_status_updates(cx);
    }
}