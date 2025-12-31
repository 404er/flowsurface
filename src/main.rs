// ============================================================================
// 条件编译属性：Release 模式下隐藏 Windows 控制台窗口
// #![cfg_attr] 是 Rust 的条件属性，根据编译配置决定是否应用
// ============================================================================
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// ============================================================================
// 模块声明：声明项目的子模块结构
// Rust 的模块系统使用 mod 关键字，每个模块对应一个文件或目录
// ============================================================================
mod audio;      // 音频播放模块
mod chart;      // 图表渲染核心模块
mod layout;     // 布局管理模块
mod logger;     // 日志系统模块
mod modal;      // 模态对话框模块
mod screen;     // 屏幕/界面模块
mod style;      // 样式和主题模块
mod widget;     // 自定义UI组件模块
mod window;     // 窗口管理模块
mod i18n;

rust_i18n::i18n!("locales", fallback = "en-US");
use rust_i18n::t;
use data::config::theme::default_theme;
use data::{layout::WindowSpec, sidebar};
use layout::{LayoutId, configuration};
use modal::{LayoutManager, SettingWindow, ThemeEditor, audio::AudioStream};
use modal::{dashboard_modal, main_dialog_modal, setting_window};
use screen::dashboard::{self, Dashboard};
use widget::{
    confirm_dialog_container,
    toast::{self, Toast},
    tooltip,
};

// iced 是 GUI 框架，使用 Elm 架构模式
use iced::{
    Alignment, Element, Subscription, Task, keyboard, padding,
    widget::{
        button, column, container, pick_list, row, rule, scrollable, text,
        tooltip::Position as TooltipPosition,
    },
};
use std::{collections::HashMap, vec};

fn main() {
    // 初始化日志系统
    logger::setup(cfg!(debug_assertions)).expect("Failed to initialize logger");

    // 在后台线程中清理旧的市场数据文件
    std::thread::spawn(data::cleanup_old_market_data);

    let _ = iced::daemon(Flowsurface::new, Flowsurface::update, Flowsurface::view)
        .settings(iced::Settings {
            antialiasing: true,  // 开启抗锯齿，使图形更平滑
            // 加载字体文件
            // Cow (Clone on Write) 是智能指针，这里使用 Borrowed 避免拷贝
            fonts: vec![
                Cow::Borrowed(style::AZERET_MONO_BYTES),  // 等宽字体
                Cow::Borrowed(style::ICONS_BYTES),        // 图标字体
            ],
            default_text_size: iced::Pixels(12.0),
            ..Default::default()  // 其余字段使用默认值（Rust 的结构体更新语法）
        })
        .title(Flowsurface::title)           // 窗口标题
        .theme(Flowsurface::theme)           // 应用主题
        .scale_factor(Flowsurface::scale_factor)  // UI 缩放系数
        .subscription(Flowsurface::subscription)  // 事件订阅（WebSocket、定时器等）
        .run();  // 阻塞运行，直到应用退出
}

/// ============================================================================
/// Flowsurface 应用程序的全局状态结构体
/// 
/// 这是整个应用的核心状态容器，遵循 Elm 架构的 Model 部分
/// 所有的 UI 状态和数据都存储在这里
/// ============================================================================
struct Flowsurface {
    /// 主窗口句柄，用于窗口操作和坐标转换
    main_window: window::Window,
    
    /// 侧边栏状态，包含交易对列表、搜索框等
    sidebar: dashboard::Sidebar,
    
    /// 布局管理器，管理多个布局配置（工作空间）
    layout_manager: LayoutManager,
    
    /// 主题编辑器状态，支持自定义主题颜色
    theme_editor: ThemeEditor,
    
    /// 音频流管理器，处理交易声音提示
    audio_stream: AudioStream,
    
    /// 确认对话框，使用 Option 表示可能不存在
    /// Option<T> 是 Rust 的标准类型，避免空指针错误
    confirm_dialog: Option<screen::ConfirmDialog<Message>>,
    
    /// 数量单位设置（基础货币 / 报价货币）
    volume_size_unit: exchange::SizeUnit,
    
    /// UI 缩放系数（0.8 - 1.5）
    ui_scale_factor: data::ScaleFactor,
    
    /// 时区设置（UTC / 本地时间）
    timezone: data::UserTimezone,
    
    /// 当前应用主题
    theme: data::Theme,
    
    /// 通知消息队列，Vec<T> 是 Rust 的动态数组（类似 ArrayList）
    /// 注意：Vec 在堆上分配，自动管理内存
    notifications: Vec<Toast>,
    
    /// 设置窗口状态和 ID
    setting_window: Option<(SettingWindow, window::Id)>,

    language: i18n::Language,
}

/// ============================================================================
/// 消息枚举 - Elm 架构的 Msg 部分
/// 
/// 定义所有可能的用户操作和系统事件
/// 这是 Rust 的和类型（Sum Type），每个变体可以携带不同的数据
/// 
/// Rust 特性说明：
/// - #[derive(Debug, Clone)] 是派生宏，自动实现 Debug 和 Clone trait
/// - Debug trait 允许使用 {:?} 格式化输出
/// - Clone trait 允许显式复制值（Rust 默认是移动语义）
/// - enum 是标签联合（Tagged Union），编译器保证类型安全
/// ============================================================================
#[derive(Debug, Clone)]
enum Message {
    /// 打开新设置窗口
    OpenNewSettingWindow,
    /// 设置窗口已打开
    SettingWindowOpened(window::Id),
    /// 设置窗口已关闭
    SettingWindowClosed(window::Id),
    /// 设置窗口消息
    SettingWindow(setting_window::Message),

    /// 侧边栏消息（嵌套消息模式）
    /// 括号内的类型表示这个变体携带的数据
    Sidebar(dashboard::sidebar::Message),
    
    /// 市场 WebSocket 事件（实时数据）
    MarketWsEvent(exchange::Event),
    
    /// Dashboard 消息，使用结构体语法的枚举变体
    /// 可以为字段添加文档注释
    Dashboard {
        /// 目标布局 ID，None 表示使用当前活动布局
        /// Option<T> 是 Rust 处理可空值的方式，编译时保证安全
        layout_id: Option<uuid::Uuid>,
        /// 实际的 Dashboard 事件
        event: dashboard::Message,
    },
    
    /// 定时器滴答事件（每 100ms 触发一次）
    /// Instant 是单调时钟，用于性能测量
    Tick(std::time::Instant),
    
    /// 窗口事件（关闭、调整大小等）
    WindowEvent(window::Event),
    
    /// 退出请求，携带所有窗口的位置和尺寸信息
    /// HashMap 是哈希表，存储键值对
    ExitRequested(HashMap<window::Id, WindowSpec>),
    
    /// 重启请求（例如切换数量单位需要重启）
    RestartRequested(HashMap<window::Id, WindowSpec>),
    
    /// 返回上一级（ESC 键）
    GoBack,
    
    /// 打开数据文件夹请求
    DataFolderRequested,
    
    /// 主题选择变更
    ThemeSelected(data::Theme),
    
    /// UI 缩放系数变更
    ScaleFactorChanged(data::ScaleFactor),
    
    /// 时区设置变更
    SetTimezone(data::UserTimezone),
    
    /// 切换历史交易数据获取（仅 Binance）
    /// bool 表示开启/关闭
    ToggleTradeFetch(bool),
    
    /// 应用数量单位设置（需要重启）
    ApplyVolumeSizeUnit(exchange::SizeUnit),
    
    /// 移除指定索引的通知
    /// usize 是平台相关的无符号整数类型（指针大小）
    RemoveNotification(usize),
    
    /// 切换对话框显示状态
    ToggleDialogModal(Option<screen::ConfirmDialog<Message>>),
    
    /// 主题编辑器消息
    ThemeEditor(modal::theme_editor::Message),
    
    /// 布局管理器消息
    Layouts(modal::layout_manager::Message),
    
    /// 音频流消息
    AudioStream(modal::audio::Message),

    // 语言切换
    LanguageChanged(i18n::Language),
}

impl Flowsurface {
    fn new() -> (Self, Task<Message>) {
        let saved_state = layout::load_saved_state();

        let (main_window_id, open_main_window) = {
            let (position, size) = saved_state.window();
            let config = window::Settings {
                size,
                position,
                exit_on_close_request: false,
                ..window::settings()
            };
            window::open(config)
        };

        let (sidebar, launch_sidebar) = dashboard::Sidebar::new(&saved_state);

        let mut state = Self {
            main_window: window::Window::new(main_window_id),
            layout_manager: saved_state.layout_manager,
            theme_editor: ThemeEditor::new(saved_state.custom_theme),
            audio_stream: AudioStream::new(saved_state.audio_cfg),
            sidebar,
            confirm_dialog: None,
            timezone: saved_state.timezone,
            ui_scale_factor: saved_state.scale_factor,
            volume_size_unit: saved_state.volume_size_unit,
            theme: saved_state.theme,
            notifications: vec![],
            setting_window: None,
            language: i18n::Language::English,
        };

        let active_layout_id = state.layout_manager.active_layout_id().unwrap_or(
            &state
                .layout_manager
                .layouts
                .first()
                .expect("No layouts available")
                .id,
        );
        let load_layout = state.load_layout(active_layout_id.unique, main_window_id);

        (
            state,
            open_main_window
                .discard()
                .chain(load_layout)
                .chain(launch_sidebar.map(Message::Sidebar)),
        )
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::MarketWsEvent(event) => {
                let main_window_id = self.main_window.id;
                let dashboard = self.active_dashboard_mut();

                match event {
                    exchange::Event::Connected(exchange) => {
                        log::info!("a stream connected to {exchange} WS");
                    }
                    exchange::Event::Disconnected(exchange, reason) => {
                        log::info!("a stream disconnected from {exchange} WS: {reason:?}");
                    }
                    exchange::Event::DepthReceived(
                        stream,
                        depth_update_t,
                        depth,
                        trades_buffer,
                    ) => {
                        let task = dashboard
                            .update_depth_and_trades(
                                &stream,
                                depth_update_t,
                                &depth,
                                &trades_buffer,
                                main_window_id,
                            )
                            .map(move |msg| Message::Dashboard {
                                layout_id: None,
                                event: msg,
                            });

                        if let Err(err) = self.audio_stream.try_play_sound(&stream, &trades_buffer)
                        {
                            log::error!("Failed to play sound: {err}");
                        }

                        return task;
                    }
                    exchange::Event::KlineReceived(stream, kline) => {
                        return dashboard
                            .update_latest_klines(&stream, &kline, main_window_id)
                            .map(move |msg| Message::Dashboard {
                                layout_id: None,
                                event: msg,
                            });
                    }
                }
            }
            Message::Tick(now) => {
                let main_window_id = self.main_window.id;

                return self
                    .active_dashboard_mut()
                    .tick(now, main_window_id)
                    .map(move |msg| Message::Dashboard {
                        layout_id: None,
                        event: msg,
                    });
            }
            Message::WindowEvent(event) => match event {
                window::Event::CloseRequested(window) => {
                    let main_window = self.main_window.id;
                    
                    // 检查是否是设置窗口的关闭请求
                    if let Some((_, window_id)) = &self.setting_window {
                        if *window_id == window {
                            return Task::done(Message::SettingWindowClosed(window));
                        }
                    }
                    
                    let dashboard = self.active_dashboard_mut();

                    if window != main_window {
                        dashboard.popout.remove(&window);
                        return window::close(window);
                    }

                    let mut active_windows = dashboard
                        .popout
                        .keys()
                        .copied()
                        .collect::<Vec<window::Id>>();
                    active_windows.push(main_window);

                    return window::collect_window_specs(active_windows, Message::ExitRequested);
                }
            },
            Message::ExitRequested(windows) => {
                self.save_state_to_disk(&windows);
                return iced::exit();
            }
            Message::RestartRequested(windows) => {
                self.save_state_to_disk(&windows);
                return self.restart();
            }
            Message::GoBack => {
                let main_window = self.main_window.id;

                if self.confirm_dialog.is_some() {
                    self.confirm_dialog = None;
                } else if self.sidebar.active_menu().is_some() {
                    self.sidebar.set_menu(None);
                } else {
                    let dashboard = self.active_dashboard_mut();

                    if dashboard.go_back(main_window) {
                        return Task::none();
                    } else if dashboard.focus.is_some() {
                        dashboard.focus = None;
                    } else {
                        self.sidebar.hide_tickers_table();
                    }
                }
            }
            Message::ThemeSelected(theme) => {
                self.theme = theme.clone();
            }
            Message::Dashboard {
                layout_id: id,
                event: msg,
            } => {
                let Some(active_layout) = self.layout_manager.active_layout_id() else {
                    log::error!("No active layout to handle dashboard message");
                    return Task::none();
                };

                let main_window = self.main_window;
                let layout_id = id.unwrap_or(active_layout.unique);

                if let Some(dashboard) = self.layout_manager.mut_dashboard(layout_id) {
                    let (main_task, event) = dashboard.update(msg, &main_window, &layout_id);

                    let additional_task = match event {
                        Some(dashboard::Event::DistributeFetchedData {
                            layout_id,
                            pane_id,
                            data,
                            stream,
                        }) => dashboard
                            .distribute_fetched_data(main_window.id, pane_id, data, stream)
                            .map(move |msg| Message::Dashboard {
                                layout_id: Some(layout_id),
                                event: msg,
                            }),
                        Some(dashboard::Event::Notification(toast)) => {
                            self.notifications.push(toast);
                            Task::none()
                        }
                        Some(dashboard::Event::ResolveStreams { pane_id, streams }) => {
                            let tickers_info = self.sidebar.tickers_info();

                            let resolved_streams =
                                streams.into_iter().try_fold(vec![], |mut acc, persist| {
                                    let resolver = |t: &exchange::Ticker| {
                                        tickers_info.get(t).and_then(|opt| *opt)
                                    };

                                    match persist.into_stream_kind(resolver) {
                                        Ok(stream) => {
                                            acc.push(stream);
                                            Ok(acc)
                                        }
                                        Err(err) => Err(format!(
                                            "Failed to resolve persisted stream: {}",
                                            err
                                        )),
                                    }
                                });

                            match resolved_streams {
                                Ok(resolved) => {
                                    if resolved.is_empty() {
                                        Task::none()
                                    } else {
                                        dashboard
                                            .resolve_streams(main_window.id, pane_id, resolved)
                                            .map(move |msg| Message::Dashboard {
                                                layout_id: None,
                                                event: msg,
                                            })
                                    }
                                }
                                Err(err) => {
                                    log::warn!("{err}",);
                                    Task::none()
                                }
                            }
                        }
                        None => Task::none(),
                    };
                      // 处理额外的 dashboard 事件
                    return main_task
                        .map(move |msg| Message::Dashboard {
                            layout_id: Some(layout_id),
                            event: msg,
                        })
                        .chain(additional_task);
                }
            }
            Message::RemoveNotification(index) => {
                if index < self.notifications.len() {
                    self.notifications.remove(index);
                }
            }
            Message::SetTimezone(tz) => {
                self.timezone = tz;
            }
            Message::ScaleFactorChanged(value) => {
                self.ui_scale_factor = value;
            }
            Message::ToggleTradeFetch(checked) => {
                self.layout_manager
                    .iter_dashboards_mut()
                    .for_each(|dashboard| {
                        dashboard.toggle_trade_fetch(checked, &self.main_window);
                    });

                if checked {
                    self.confirm_dialog = None;
                }
            }
            Message::ToggleDialogModal(dialog) => {
                self.confirm_dialog = dialog;
            }
            Message::Layouts(message) => {
                let action = self.layout_manager.update(message);

                match action {
                    Some(modal::layout_manager::Action::Select(layout)) => {
                        let active_popout_keys = self
                            .active_dashboard()
                            .popout
                            .keys()
                            .copied()
                            .collect::<Vec<_>>();

                        let window_tasks = Task::batch(
                            active_popout_keys
                                .iter()
                                .map(|&popout_id| window::close::<window::Id>(popout_id))
                                .collect::<Vec<_>>(),
                        )
                        .discard();

                        let old_layout_id = self
                            .layout_manager
                            .active_layout_id()
                            .as_ref()
                            .map(|layout| layout.unique);

                        return window::collect_window_specs(
                            active_popout_keys,
                            dashboard::Message::SavePopoutSpecs,
                        )
                        .map(move |msg| Message::Dashboard {
                            layout_id: old_layout_id,
                            event: msg,
                        })
                        .chain(window_tasks)
                        .chain(self.load_layout(layout, self.main_window.id));
                    }
                    Some(modal::layout_manager::Action::Clone(id)) => {
                        let manager = &mut self.layout_manager;

                        let source_data = manager.get(id).map(|layout| {
                            (
                                layout.id.name.clone(),
                                layout.id.unique,
                                data::Dashboard::from(&layout.dashboard),
                            )
                        });

                        if let Some((name, old_id, ser_dashboard)) = source_data {
                            let new_uid = uuid::Uuid::new_v4();
                            let new_layout = LayoutId {
                                unique: new_uid,
                                name: manager.ensure_unique_name(&name, new_uid),
                            };

                            let mut popout_windows = Vec::new();

                            for (pane, window_spec) in &ser_dashboard.popout {
                                let configuration = configuration(pane.clone());
                                popout_windows.push((configuration, *window_spec));
                            }

                            let dashboard = Dashboard::from_config(
                                configuration(ser_dashboard.pane.clone()),
                                popout_windows,
                                old_id,
                            );

                            manager.insert_layout(new_layout.clone(), dashboard);
                        }
                    }
                    None => {}
                }
            }
            Message::AudioStream(message) => self.audio_stream.update(message),
            Message::DataFolderRequested => {
                if let Err(err) = data::open_data_folder() {
                    self.notifications
                        .push(Toast::error(format!("Failed to open data folder: {err}")));
                }
            }
            // 打开新设置窗口
            Message::OpenNewSettingWindow => {
                // 如果设置窗口已经打开，则不重复打开
                if self.setting_window.is_some() {
                    return Task::none();
                }
                
                // 使用 iced 的 window::open 打开新窗口
                let (_id, open_task) = window::open(window::Settings {
                    size: iced::Size::new(600.0, 400.0),
                    position: window::Position::Centered,
                    exit_on_close_request: false,
                    ..Default::default()
                });
                
                // 返回任务并映射到 SettingWindowOpened 消息
                return open_task.map(Message::SettingWindowOpened);
            }
            Message::SettingWindowOpened(id) => {
                // 创建新的设置窗口实例
                self.setting_window = Some((SettingWindow::new(), id));
            }
            Message::SettingWindow(msg) => {
                // 处理设置窗口的消息
                if let Some((window, id)) = &mut self.setting_window {
                    if let Some(action) = window.update(msg) {
                        match action {
                            setting_window::Action::Close => {
                                return Task::done(Message::SettingWindowClosed(id.clone()));
                            }
                            setting_window::Action::ThemeChanged(theme) => {
                                return Task::done(Message::ThemeSelected(data::Theme(theme.into()))); 
                            }
                            setting_window::Action::TimezoneChanged(timezone) => {
                                return Task::done(Message::SetTimezone(timezone));
                            }
                            setting_window::Action::OpenThemeEditor => {
                                // todo 主题编辑
                                return Task::none();
                            }
                            setting_window::Action::ScaleFactorChanged(scale_factor) => {
                                return Task::done(Message::ScaleFactorChanged(scale_factor));
                            }
                            setting_window::Action::LanguageChanged(language) => {
                                return Task::done(Message::LanguageChanged(language));
                            }
                        }
                    }
                }
            }
            Message::SettingWindowClosed(id) => {
                // 清除设置窗口的状态
                if let Some((_, window_id)) = &self.setting_window {
                    if *window_id == id {
                        self.setting_window = None;
                    }
                }
                
                // 关闭窗口
                return window::close(id);
            }
            Message::ThemeEditor(msg) => {
                let action = self.theme_editor.update(msg, &self.theme.clone().into());

                match action {
                    Some(modal::theme_editor::Action::Exit) => {
                        self.sidebar.set_menu(Some(sidebar::Menu::Settings));
                    }
                    Some(modal::theme_editor::Action::UpdateTheme(theme)) => {
                        self.theme = data::Theme(theme);

                        let main_window = self.main_window.id;

                        self.active_dashboard_mut()
                            .invalidate_all_panes(main_window);
                    }
                    None => {}
                }
            }
            Message::Sidebar(message) => {
                let (task, action) = self.sidebar.update(message);

                match action {
                    Some(dashboard::sidebar::Action::TickerSelected(ticker_info, content)) => {
                        let main_window_id = self.main_window.id;

                        let task = {
                            if let Some(kind) = content {
                                self.active_dashboard_mut().init_focused_pane(
                                    main_window_id,
                                    ticker_info,
                                    kind,
                                )
                            } else {
                                self.active_dashboard_mut()
                                    .switch_tickers_in_group(main_window_id, ticker_info)
                            }
                        };

                        return task.map(move |msg| Message::Dashboard {
                            layout_id: None,
                            event: msg,
                        });
                    }
                    Some(dashboard::sidebar::Action::ErrorOccurred(err)) => {
                        self.notifications.push(Toast::error(err.to_string()));
                    }
                    None => {}
                }

                return task.map(Message::Sidebar);
            }
            Message::ApplyVolumeSizeUnit(pref) => {
                self.volume_size_unit = pref;
                self.confirm_dialog = None;

                let mut active_windows: Vec<window::Id> =
                    self.active_dashboard().popout.keys().copied().collect();
                active_windows.push(self.main_window.id);

                return window::collect_window_specs(active_windows, Message::RestartRequested);
            }
            Message::LanguageChanged(lang) => {
                i18n::set_language(lang);
                self.language = lang;
            }
        }
        Task::none()
    }

    fn view(&self, id: window::Id) -> Element<'_, Message> {
        let dashboard = self.active_dashboard();
        let sidebar_pos = self.sidebar.position();

        let tickers_table = &self.sidebar.tickers_table;

        let content = if id == self.main_window.id {
            let sidebar_view = self
                .sidebar
                .view(self.audio_stream.volume())
                .map(Message::Sidebar);

            let dashboard_view = dashboard
                .view(&self.main_window, tickers_table, self.timezone)
                .map(move |msg| Message::Dashboard {
                    layout_id: None,
                    event: msg,
                });

            let header_title = {
                #[cfg(target_os = "macos")]
                {
                    iced::widget::center(
                        text("FLOWSURFACE")
                            .font(iced::Font {
                                weight: iced::font::Weight::Bold,
                                ..Default::default()
                            })
                            .size(16)
                            .style(style::title_text),
                    )
                    .height(20)
                    .align_y(Alignment::Center)
                    .padding(padding::top(4))
                }
                #[cfg(not(target_os = "macos"))]
                {
                    column![]
                }
            };

            let base = column![
                header_title,
                match sidebar_pos {
                    sidebar::Position::Left => row![sidebar_view, dashboard_view,],
                    sidebar::Position::Right => row![dashboard_view, sidebar_view],
                }
                .spacing(4)
                .padding(8),
            ];

            if let Some(menu) = self.sidebar.active_menu() {
                self.view_with_modal(base.into(), dashboard, menu)
            } else {
                base.into()
            }
        } else if let Some((window, window_id)) = &self.setting_window {
            // 设置窗口的视图
            if *window_id == id {
                return window.view(
                    &self.theme,
                    &self.theme_editor,
                    self.timezone,
                    self.volume_size_unit,
                    self.ui_scale_factor,
                    // self.sidebar.position(),
                ).map(Message::SettingWindow);
            }
            
            // 如果不是设置窗口，继续检查其他窗口
            container(
                dashboard
                    .view_window(id, &self.main_window, tickers_table, self.timezone)
                    .map(move |msg| Message::Dashboard {
                        layout_id: None,
                        event: msg,
                    }),
            )
            .padding(padding::top(style::TITLE_PADDING_TOP))
            .into()
        } else {
            container(
                dashboard
                    .view_window(id, &self.main_window, tickers_table, self.timezone)
                    .map(move |msg| Message::Dashboard {
                        layout_id: None,
                        event: msg,
                    }),
            )
            .padding(padding::top(style::TITLE_PADDING_TOP))
            .into()
        };

        toast::Manager::new(
            content,
            &self.notifications,
            match sidebar_pos {
                sidebar::Position::Left => Alignment::Start,
                sidebar::Position::Right => Alignment::End,
            },
            Message::RemoveNotification,
        )
        .into()
    }

    fn theme(&self, _window: window::Id) -> iced_core::Theme {
        self.theme.clone().into()
    }

    fn title(&self, window: window::Id) -> String {
        // 检查是否是设置窗口
        if let Some((_, window_id)) = &self.setting_window {
            if *window_id == window {
                return t!("settings.title").to_string();
            }
        }
        
        if let Some(id) = self.layout_manager.active_layout_id() {
            format!("Flowsurface [{}]", id.name)
        } else {
            "Flowsurface".to_string()
        }
    }

    fn scale_factor(&self, _window: window::Id) -> f32 {
        self.ui_scale_factor.into()
    }

    fn subscription(&self) -> Subscription<Message> {
        let window_events = window::events().map(Message::WindowEvent);
        let sidebar = self.sidebar.subscription().map(Message::Sidebar);

        let exchange_streams = self
            .active_dashboard()
            .market_subscriptions()
            .map(Message::MarketWsEvent);

        let tick = iced::time::every(std::time::Duration::from_millis(100)).map(Message::Tick);

        let hotkeys = keyboard::listen().filter_map(|event| {
            let keyboard::Event::KeyPressed { key, .. } = event else {
                return None;
            };
            match key {
                keyboard::Key::Named(keyboard::key::Named::Escape) => Some(Message::GoBack),
                _ => None,
            }
        });

        Subscription::batch(vec![
            exchange_streams,
            sidebar,
            window_events,
            tick,
            hotkeys,
        ])
    }

    fn active_dashboard(&self) -> &Dashboard {
        let active_layout = self
            .layout_manager
            .active_layout_id()
            .expect("No active layout");
        self.layout_manager
            .get(active_layout.unique)
            .map(|layout| &layout.dashboard)
            .expect("No active dashboard")
    }

    fn active_dashboard_mut(&mut self) -> &mut Dashboard {
        let active_layout = self
            .layout_manager
            .active_layout_id()
            .expect("No active layout");
        self.layout_manager
            .get_mut(active_layout.unique)
            .map(|layout| &mut layout.dashboard)
            .expect("No active dashboard")
    }

    fn load_layout(&mut self, layout_uid: uuid::Uuid, main_window: window::Id) -> Task<Message> {
        match self.layout_manager.set_active_layout(layout_uid) {
            Ok(layout) => {
                layout
                    .dashboard
                    .load_layout(main_window)
                    .map(move |msg| Message::Dashboard {
                        layout_id: Some(layout_uid),
                        event: msg,
                    })
            }
            Err(err) => {
                log::error!("Failed to set active layout: {}", err);
                Task::none()
            }
        }
    }

    fn view_with_modal<'a>(
        &'a self,
        base: Element<'a, Message>,
        dashboard: &'a Dashboard,
        menu: sidebar::Menu,
    ) -> Element<'a, Message> {
        let sidebar_pos = self.sidebar.position();

        match menu {
            sidebar::Menu::Settings => {
                let settings_modal = {
                    let theme_picklist = {
                        let mut themes: Vec<iced::Theme> = iced_core::Theme::ALL.to_vec();

                        let default_theme = iced_core::Theme::Custom(default_theme().into());
                        themes.push(default_theme);

                        if let Some(custom_theme) = &self.theme_editor.custom_theme {
                            themes.push(custom_theme.clone());
                        }

                        pick_list(themes, Some(self.theme.0.clone()), |theme| {
                            Message::ThemeSelected(data::Theme(theme))
                        })
                    };

                    let toggle_theme_editor = button(text("Theme editor")).on_press(
                        Message::Sidebar(dashboard::sidebar::Message::ToggleSidebarMenu(Some(
                            sidebar::Menu::ThemeEditor,
                        ))),
                    );

                    let timezone_picklist = pick_list(
                        [data::UserTimezone::Utc, data::UserTimezone::Local],
                        Some(self.timezone),
                        Message::SetTimezone,
                    );

                    let size_in_quote_currency_checkbox = {
                        let is_active = match self.volume_size_unit {
                            exchange::SizeUnit::Quote => true,
                            exchange::SizeUnit::Base => false,
                        };

                        let checkbox = iced::widget::checkbox(is_active)
                            .label("Size in quote currency")
                            .on_toggle(|checked| {
                                let on_dialog_confirm = Message::ApplyVolumeSizeUnit(if checked {
                                    exchange::SizeUnit::Quote
                                } else {
                                    exchange::SizeUnit::Base
                                });

                                let confirm_dialog = screen::ConfirmDialog::new(
                                    "Changing size display currency requires application restart"
                                        .to_string(),
                                    Box::new(on_dialog_confirm.clone()),
                                )
                                .with_confirm_btn_text("Restart now".to_string());

                                Message::ToggleDialogModal(Some(confirm_dialog))
                            });

                        tooltip(
                            checkbox,
                            Some(
                                "Display sizes/volumes in quote currency (USD)\nHas no effect on inverse perps or open interest",
                            ),
                            TooltipPosition::Top,
                        )
                    };

                    let sidebar_pos = pick_list(
                        [sidebar::Position::Left, sidebar::Position::Right],
                        Some(sidebar_pos),
                        |pos| {
                            Message::Sidebar(dashboard::sidebar::Message::SetSidebarPosition(pos))
                        },
                    );

                    let scale_factor = {
                        let current_value: f32 = self.ui_scale_factor.into();

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
                        .style(style::modal_container)
                    };

                    let trade_fetch_checkbox = {
                        let is_active = exchange::fetcher::is_trade_fetch_enabled();

                        let checkbox = iced::widget::checkbox(is_active)
                            .label("Fetch trades (Binance)")
                            .on_toggle(|checked| {
                                if checked {
                                    let confirm_dialog = screen::ConfirmDialog::new(
                                        "This might be unreliable and take some time to complete. Proceed?"
                                            .to_string(),
                                        Box::new(Message::ToggleTradeFetch(true)),
                                    );
                                    Message::ToggleDialogModal(Some(confirm_dialog))
                                } else {
                                    Message::ToggleTradeFetch(false)
                                }
                            });

                        tooltip(
                            checkbox,
                            Some("Try to fetch trades for footprint charts"),
                            TooltipPosition::Top,
                        )
                    };

                    let open_data_folder = {
                        let button =
                            button(text("Open data folder")).on_press(Message::DataFolderRequested);

                        tooltip(
                            button,
                            Some("Open the folder where the data & config is stored"),
                            TooltipPosition::Top,
                        )
                    };

                    let open_new_window_test = {
                        let button = button(text("Open new window test")).on_press(Message::OpenNewSettingWindow);
                        tooltip(
                            button,
                            Some("Open a new window for testing"),
                            TooltipPosition::Top,
                        )
                    };

                    let column_content = split_column![
                        column![open_new_window_test,].spacing(8),
                        column![open_data_folder,].spacing(8),
                        column![text("Sidebar position").size(14), sidebar_pos,].spacing(12),
                        column![text("Time zone").size(14), timezone_picklist,].spacing(12),
                        column![text("Market data").size(14), size_in_quote_currency_checkbox,].spacing(12),
                        column![text("Theme").size(14), theme_picklist,].spacing(12),
                        column![text("Interface scale").size(14), scale_factor,].spacing(12),
                        column![
                            text("Experimental").size(14),
                            column![trade_fetch_checkbox, toggle_theme_editor,].spacing(8),
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
                        .max_width(240)
                        .padding(24)
                        .style(style::dashboard_modal)
                };

                let (align_x, padding) = match sidebar_pos {
                    sidebar::Position::Left => (Alignment::Start, padding::left(44).bottom(4)),
                    sidebar::Position::Right => (Alignment::End, padding::right(44).bottom(4)),
                };

                let base_content = dashboard_modal(
                    base,
                    settings_modal,
                    Message::Sidebar(dashboard::sidebar::Message::ToggleSidebarMenu(None)),
                    padding,
                    Alignment::End,
                    align_x,
                );

                if let Some(dialog) = &self.confirm_dialog {
                    let dialog_content =
                        confirm_dialog_container(dialog.clone(), Message::ToggleDialogModal(None));

                    main_dialog_modal(
                        base_content,
                        dialog_content,
                        Message::ToggleDialogModal(None),
                    )
                } else {
                    base_content
                }
            }
            sidebar::Menu::Layout => {
                let main_window = self.main_window.id;

                let manage_pane = if let Some((window_id, pane_id)) = dashboard.focus {
                    let selected_pane_str =
                        if let Some(state) = dashboard.get_pane(main_window, window_id, pane_id) {
                            let link_group_name: String =
                                state.link_group.as_ref().map_or_else(String::new, |g| {
                                    " - Group ".to_string() + &g.to_string()
                                });

                            state.content.to_string() + &link_group_name
                        } else {
                            "".to_string()
                        };

                    let is_main_window = window_id == main_window;

                    let reset_pane_button = {
                        let btn = button(text("Reset").align_x(Alignment::Center))
                            .width(iced::Length::Fill);
                        if is_main_window {
                            let dashboard_msg = Message::Dashboard {
                                layout_id: None,
                                event: dashboard::Message::Pane(
                                    main_window,
                                    dashboard::pane::Message::ReplacePane(pane_id),
                                ),
                            };

                            btn.on_press(dashboard_msg)
                        } else {
                            btn
                        }
                    };
                    // let split_pane_button = {
                    //     let btn = button(text("Split").align_x(Alignment::Center))
                    //         .width(iced::Length::Fill);
                    //     if is_main_window {
                    //         let dashboard_msg = Message::Dashboard {
                    //             layout_id: None,
                    //             event: dashboard::Message::Pane(
                    //                 main_window,
                    //                 dashboard::pane::Message::SplitPane(
                    //                     pane_grid::Axis::Horizontal,
                    //                     pane_id,
                    //                 ),
                    //             ),
                    //         };
                    //         btn.on_press(dashboard_msg)
                    //     } else {
                    //         btn
                    //     }
                    // };

                    column![
                        text(selected_pane_str),
                        row![
                            tooltip(
                                reset_pane_button,
                                if is_main_window {
                                    Some("Reset selected pane")
                                } else {
                                    None
                                },
                                TooltipPosition::Top,
                            ),
                            // tooltip(
                            //     split_pane_button,
                            //     if is_main_window {
                            //         Some("Split selected pane horizontally")
                            //     } else {
                            //         None
                            //     },
                            //     TooltipPosition::Top,
                            // ),
                        ]
                        .spacing(8)
                    ]
                    .spacing(8)
                } else {
                    column![text("No pane selected"),].spacing(8)
                };

                let manage_layout_modal = {
                    let col = column![
                        manage_pane,
                        rule::horizontal(1.0).style(style::split_ruler),
                        self.layout_manager.view().map(Message::Layouts)
                    ];

                    container(col.align_x(Alignment::Center).spacing(20))
                        .width(260)
                        .padding(24)
                        .style(style::dashboard_modal)
                };

                let (align_x, padding) = match sidebar_pos {
                    sidebar::Position::Left => (Alignment::Start, padding::left(44).top(40)),
                    sidebar::Position::Right => (Alignment::End, padding::right(44).top(40)),
                };

                dashboard_modal(
                    base,
                    manage_layout_modal,
                    Message::Sidebar(dashboard::sidebar::Message::ToggleSidebarMenu(None)),
                    padding,
                    Alignment::Start,
                    align_x,
                )
            }
            sidebar::Menu::Audio => {
                let (align_x, padding) = match sidebar_pos {
                    sidebar::Position::Left => (Alignment::Start, padding::left(44).top(76)),
                    sidebar::Position::Right => (Alignment::End, padding::right(44).top(76)),
                };

                let depth_streams_list = dashboard.streams.depth_streams(None);

                dashboard_modal(
                    base,
                    self.audio_stream
                        .view(depth_streams_list)
                        .map(Message::AudioStream),
                    Message::Sidebar(dashboard::sidebar::Message::ToggleSidebarMenu(None)),
                    padding,
                    Alignment::Start,
                    align_x,
                )
            }
            sidebar::Menu::ThemeEditor => {
                let (align_x, padding) = match sidebar_pos {
                    sidebar::Position::Left => (Alignment::Start, padding::left(44).bottom(4)),
                    sidebar::Position::Right => (Alignment::End, padding::right(44).bottom(4)),
                };

                dashboard_modal(
                    base,
                    self.theme_editor
                        .view(&self.theme.0)
                        .map(Message::ThemeEditor),
                    Message::Sidebar(dashboard::sidebar::Message::ToggleSidebarMenu(None)),
                    padding,
                    Alignment::End,
                    align_x,
                )
            }
        }
    }

    fn save_state_to_disk(&mut self, windows: &HashMap<window::Id, WindowSpec>) {
        self.active_dashboard_mut()
            .popout
            .iter_mut()
            .for_each(|(id, (_, window_spec))| {
                if let Some(new_window_spec) = windows.get(id) {
                    *window_spec = *new_window_spec;
                }
            });

        self.sidebar.sync_tickers_table_settings();

        let mut ser_layouts = vec![];
        for layout in &self.layout_manager.layouts {
            if let Some(layout) = self.layout_manager.get(layout.id.unique) {
                let serialized_dashboard = data::Dashboard::from(&layout.dashboard);
                ser_layouts.push(data::Layout {
                    name: layout.id.name.clone(),
                    dashboard: serialized_dashboard,
                });
            }
        }

        let layouts = data::Layouts {
            layouts: ser_layouts,
            active_layout: self
                .layout_manager
                .active_layout_id()
                .map(|layout| layout.name.to_string())
                .clone(),
        };

        let main_window_spec = windows
            .iter()
            .find(|(id, _)| **id == self.main_window.id)
            .map(|(_, spec)| *spec);

        let audio_cfg = data::AudioStream::from(&self.audio_stream);

        let state = data::State::from_parts(
            layouts,
            self.theme.clone(),
            self.theme_editor.custom_theme.clone().map(data::Theme),
            main_window_spec,
            self.timezone,
            self.sidebar.state.clone(),
            self.ui_scale_factor,
            audio_cfg,
            self.volume_size_unit,
        );

        match serde_json::to_string(&state) {
            Ok(layout_str) => {
                let file_name = data::SAVED_STATE_PATH;
                if let Err(e) = data::write_json_to_file(&layout_str, file_name) {
                    log::error!("Failed to write layout state to file: {}", e);
                } else {
                    log::info!("Persisted state to {file_name}");
                }
            }
            Err(e) => log::error!("Failed to serialize layout: {}", e),
        }
    }

    fn restart(&mut self) -> Task<Message> {
        let mut windows_to_close: Vec<window::Id> =
            self.active_dashboard().popout.keys().copied().collect();
        windows_to_close.push(self.main_window.id);

        let close_windows = Task::batch(
            windows_to_close
                .into_iter()
                .map(window::close)
                .collect::<Vec<_>>(),
        );

        let (new_state, init_task) = Flowsurface::new();
        *self = new_state;

        close_windows.chain(init_task)
    }
}
