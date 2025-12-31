use iced::{
    Alignment, Element,
    widget::{button, column, container, text, pick_list, scrollable, row},
};
use crate::widget::{
    confirm_dialog_container,
    toast::{self, Toast},
    tooltip,
};
use crate::split_column;
use crate::modal::ThemeEditor;
use data::config::theme::default_theme;
use crate::i18n::{self, t};

/// 设置窗口消息
#[derive(Debug, Clone)]
pub enum Message {
    ThemeSelected(data::Theme),
    OpenThemeEditor,
    SetTimezone(data::UserTimezone),
    // ToggleVolumeSizeUnit(bool),
    ScaleFactorChanged(data::ScaleFactor),
    // ToggleTradeFetch(bool),
    // OpenDataFolder,
    CloseRequested,
    LanguageChanged(i18n::Language),
}

/// 设置窗口返回给父组件的动作
#[derive(Debug, Clone)]
pub enum Action {
    ThemeChanged(data::Theme),
    OpenThemeEditor,
    TimezoneChanged(data::UserTimezone),
    // RequestVolumeSizeUnitChange(exchange::SizeUnit), // 需要确认对话框
    ScaleFactorChanged(data::ScaleFactor),
    // TradeFetchToggled(bool),
    // DataFolderRequested,
    Close,
    LanguageChanged(i18n::Language),
}

/// 设置窗口状态
pub struct SettingWindow {
}

impl SettingWindow {
    /// 创建新的设置窗口
    pub fn new() -> Self {
        Self {
            
        }
    }

    /// 更新设置窗口状态
    pub fn update(&mut self, message: Message) -> Option<Action> {
        match message {
            Message::CloseRequested => {
                Some(Action::Close)
            }
            Message::SetTimezone(timezone) => {
                Some(Action::TimezoneChanged(timezone))
            }
            Message::ThemeSelected(theme) => {
                Some(Action::ThemeChanged(theme))
            }
            Message::OpenThemeEditor => {
                Some(Action::OpenThemeEditor)
            }
            Message::ScaleFactorChanged(scale_factor) => {
                Some(Action::ScaleFactorChanged(scale_factor))
            }
            Message::LanguageChanged(language) => {
                Some(Action::LanguageChanged(language))
            }
        }
    }

    /// 渲染设置窗口视图
    pub fn view(
        &self,
        theme: &data::Theme,
        theme_editor: &ThemeEditor,
        timezone: data::UserTimezone,
        volume_size_unit: exchange::SizeUnit,
        ui_scale_factor: data::ScaleFactor,
    ) -> Element<'_, Message> {
        let settings_modal = {
            let theme_picklist = {
                let mut themes: Vec<iced::Theme> = iced_core::Theme::ALL.to_vec();

                let default_theme = iced_core::Theme::Custom(default_theme().into());
                themes.push(default_theme);

                if let Some(custom_theme) = &theme_editor.custom_theme {
                    themes.push(custom_theme.clone());
                }

                pick_list(themes, Some(theme.0.clone()), |theme| {
                    Message::ThemeSelected(data::Theme(theme))
                })
            };

            let toggle_theme_editor = button(text("Theme editor")).on_press( Message::OpenThemeEditor);

            let timezone_picklist = pick_list(
                [data::UserTimezone::Utc, data::UserTimezone::Local],
                Some(timezone),
                Message::SetTimezone,
            );

            let current_lang = i18n::Language::from_code(i18n::current_language());
            let language_picker = pick_list(
                [
                    i18n::Language::English,
                    i18n::Language::SimplifiedChinese,
                ],
                Some(current_lang),
                Message::LanguageChanged,
            );

            // let size_in_quote_currency_checkbox = {
            //     let is_active = match self.volume_size_unit {
            //         exchange::SizeUnit::Quote => true,
            //         exchange::SizeUnit::Base => false,
            //     };

            //     let checkbox = iced::widget::checkbox(is_active)
            //         .label("Size in quote currency")
            //         .on_toggle(|checked| {
            //             let on_dialog_confirm = Message::ApplyVolumeSizeUnit(if checked {
            //                 exchange::SizeUnit::Quote
            //             } else {
            //                 exchange::SizeUnit::Base
            //             });

            //             let confirm_dialog = screen::ConfirmDialog::new(
            //                 "Changing size display currency requires application restart"
            //                     .to_string(),
            //                 Box::new(on_dialog_confirm.clone()),
            //             )
            //             .with_confirm_btn_text("Restart now".to_string());

            //             Message::ToggleDialogModal(Some(confirm_dialog))
            //         });

            //     tooltip(
            //         checkbox,
            //         Some(
            //             "Display sizes/volumes in quote currency (USD)\nHas no effect on inverse perps or open interest",
            //         ),
            //         TooltipPosition::Top,
            //     )
            // };

            let scale_factor = {
                let current_value: f32 = ui_scale_factor.into();

                let decrease_btn = if current_value > data::config::MIN_SCALE {
                    button(text("-"))
                        .on_press(Message::ScaleFactorChanged((current_value - 0.1).into()))
                } else {
                    button(text("-"))
                };

                let increase_btn = if current_value < data::config::MAX_SCALE {
                    button(text("+"))
                        .on_press(Message::ScaleFactorChanged((current_value + 0.1).into()))
                } else {
                    button(text("+"))
                };

                container(
                    row![
                        decrease_btn,
                        text(format!("{:.0}%", current_value * 100.0)).size(14),
                        increase_btn,
                    ]
                    .align_y(Alignment::Center)
                    .spacing(8)
                    .padding(4),
                )
                .style(crate::style::modal_container)
            };

            // let trade_fetch_checkbox = {
            //     let is_active = exchange::fetcher::is_trade_fetch_enabled();

            //     let checkbox = iced::widget::checkbox(is_active)
            //         .label("Fetch trades (Binance)")
            //         .on_toggle(|checked| {
            //             if checked {
            //                 let confirm_dialog = screen::ConfirmDialog::new(
            //                     "This might be unreliable and take some time to complete. Proceed?"
            //                         .to_string(),
            //                     Box::new(Message::ToggleTradeFetch(true)),
            //                 );
            //                 Message::ToggleDialogModal(Some(confirm_dialog))
            //             } else {
            //                 Message::ToggleTradeFetch(false)
            //             }
            //         });

            //     tooltip(
            //         checkbox,
            //         Some("Try to fetch trades for footprint charts"),
            //         TooltipPosition::Top,
            //     )
            // };

            // let open_data_folder = {
            //     let button =
            //         button(text("Open data folder")).on_press(Message::DataFolderRequested);

            //     tooltip(
            //         button,
            //         Some("Open the folder where the data & config is stored"),
            //         TooltipPosition::Top,
            //     )
            // };

            let column_content = split_column![
                // column![open_data_folder,].spacing(8),
                column![text(t!("settings.timezone")).size(14), timezone_picklist,].spacing(12),
                column![text(t!("settings.language")).size(14), language_picker,].spacing(12),
                // column![text("Market data").size(14), size_in_quote_currency_checkbox,].spacing(12),
                
                column![text(t!("settings.theme")).size(14), theme_picklist,].spacing(12),
                column![text(t!("settings.interface_scale")).size(14), scale_factor,].spacing(12),
                column![
                    text("Experimental").size(14),
                    // column![trade_fetch_checkbox, toggle_theme_editor,].spacing(8),
                    column![toggle_theme_editor,].spacing(8),
                ]
                .spacing(12),
                ; spacing = 16, align_x = Alignment::Start
            ];

            let content = scrollable::Scrollable::with_direction(
                column_content,
                scrollable::Direction::Vertical(
                    scrollable::Scrollbar::new().width(8).scroller_width(6),
                ),
            );

            container(content)
                .align_x(Alignment::Start)
                // .max_width(iced::Fill)
                .padding(24)
                .style(crate::style::dashboard_modal)
        };
        
        settings_modal.center(iced::Fill).into()
    }
}

impl Default for SettingWindow {
    fn default() -> Self {
        Self::new()
    }
}

